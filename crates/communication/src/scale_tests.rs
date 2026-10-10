//! Real filesystem load, synthetic logical identities: no workers or model calls.
use super::*;
use std::{io::Read, time::Instant};

fn proposal(agent: usize, slot: usize) -> Proposal {
    Proposal {
        schema_version: 1,
        id: format!("p{slot}"),
        kind: Kind::Finding,
        text: format!("synthetic member {agent} slot {slot}; no research finding"),
        topics: vec!["recovery".into()],
        references: vec![],
        reply_to: None,
        recipients: vec![],
    }
}

fn exercise(count: usize) -> Result<serde_json::Value> {
    let started = Instant::now();
    let state = tempfile::tempdir()?;
    let board = Board::new(state.path())?;
    let cells = count.div_ceil(32);
    let mut members = Vec::with_capacity(count);
    let mut endpoints = Vec::with_capacity(count);
    for id in 0..count {
        let member = HostMember {
            cohort_id: storage::digest(format!("test-mission/cell/{}", id / 32).as_bytes()),
            task_id: format!("logical-{id}"),
            run_id: format!("logical-{id}-attempt-1"),
            authority_sha256: storage::digest(format!("frozen-{id}").as_bytes()),
        };
        let endpoint = board.register(&member, 0)?;
        storage::write(&endpoint.outbox.join("00.json"), &proposal(id, 0))?;
        members.push(member);
        endpoints.push(endpoint);
    }
    let registration_ms = started.elapsed().as_millis();
    // Retaining a stale file descriptor must not block host publication.
    let mut slow_snapshot = File::open(endpoints[0].view.join("board.json"))?;
    let phase = Instant::now();
    let mut accepted = 0;
    let mut max_scanned = 0;
    for cell in 0..cells {
        let poll = board.poll(&members[cell * 32].cohort_id, 1)?;
        assert!(poll.scanned_slots <= POLL_SLOTS);
        assert_eq!(poll.accepted, (count - cell * 32).min(32));
        accepted += poll.accepted;
        max_scanned = max_scanned.max(poll.scanned_slots);
    }
    assert_eq!(accepted, count, "the tail cell must not be starved");
    let initial_poll_ms = phase.elapsed().as_millis();
    let selection = Selection {
        topics: vec!["recovery".into()],
        query: String::new(),
    };
    let phase = Instant::now();
    let mut max_context_bytes = 0;
    let mut max_selected_messages = 0;
    let mut context_reads = 0;
    let mut read_context = |member: &HostMember, now| -> Result<()> {
        let context = board.context(member, &selection, now)?;
        assert!(context.text.len() <= MAX_CONTEXT_BYTES);
        assert!(context.message_ids.len() <= 4);
        assert!(!context.message_ids.is_empty());
        max_context_bytes = max_context_bytes.max(context.text.len());
        max_selected_messages = max_selected_messages.max(context.message_ids.len());
        context_reads += 1;
        Ok(())
    };
    // Ten percent never consume, another ten percent only consume after the storm.
    for (id, member) in members.iter().enumerate() {
        if id % 10 > 1 {
            read_context(member, 1)?;
        }
    }
    let initial_context_ms = phase.elapsed().as_millis();
    let first = &members[0];
    let mut impostor = first.clone();
    impostor.cohort_id = members[32].cohort_id.clone();
    assert!(board.context(&impostor, &selection, 1).is_err());
    assert!(board.register(&impostor, 1).is_err());

    // Fixed slots bound poll work even if every member of one cell is noisy.
    for (id, endpoint) in endpoints.iter().take(32).enumerate() {
        for slot in 1..OUTBOX_SLOTS {
            let proposal = if id == 0 && slot == 1 {
                proposal(0, 0)
            } else {
                proposal(id, slot)
            };
            storage::write(&endpoint.outbox.join(format!("{slot:02}.json")), &proposal)?;
        }
    }
    let mut noisy_accepted = 0;
    let mut noisy_rejected = 0;
    let mut noisy_duplicates = 0;
    for now in 2..=17 {
        let poll = board.poll(&first.cohort_id, now)?;
        assert!(poll.scanned_slots <= POLL_SLOTS);
        max_scanned = max_scanned.max(poll.scanned_slots);
        noisy_accepted += poll.accepted;
        noisy_rejected += poll.rejected;
        noisy_duplicates += poll.duplicates;
    }
    let view = board.view(&first.cohort_id, 17)?;
    let lifetime_total = view["accepted_total"].as_u64().unwrap();
    assert_eq!(lifetime_total, MAX_COHORT_MESSAGES as u64);
    assert!(view["serialized_view_bytes"].as_u64().unwrap() <= MAX_STATE_BYTES as u64);
    assert_eq!(noisy_duplicates, 1);
    assert!(noisy_rejected > 0);
    assert_eq!(32 + noisy_accepted, MAX_COHORT_MESSAGES);

    let phase = Instant::now();
    for (id, member) in members.iter().enumerate() {
        if id % 10 == 1 {
            read_context(member, 17)?;
        }
    }
    let delayed_context_ms = phase.elapsed().as_millis();
    assert_eq!(context_reads, count * 9 / 10);
    let mut stale = String::new();
    slow_snapshot.read_to_string(&mut stale)?;
    assert!(
        serde_json::from_str::<serde_json::Value>(&stale)?["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!view["messages"].as_array().unwrap().is_empty());

    let reopened = Board::new(state.path())?;
    reopened.register(first, 18)?;
    for now in 18..=33 {
        assert_eq!(reopened.poll(&first.cohort_id, now)?.accepted, 0);
    }
    assert_eq!(
        reopened.view(&first.cohort_id, 33)?["accepted_total"],
        lifetime_total
    );
    let expired_at = TTL_SECONDS + 100;
    assert!(
        reopened
            .context(first, &selection, expired_at)?
            .message_ids
            .is_empty()
    );
    assert_eq!(
        reopened.view(&first.cohort_id, expired_at)?["accepted_total"],
        lifetime_total
    );

    let mut all = BTreeSet::new();
    for shard in 0..=255u8 {
        let mut after: Option<String> = None;
        loop {
            let page = reopened.cohorts(shard, after.as_deref(), 2)?;
            let rows = page["cohorts"].as_array().unwrap();
            assert!(rows.len() <= 2);
            for row in rows {
                assert!(all.insert(row["cohort_id"].as_str().unwrap().to_owned()));
            }
            after = page["next_after"].as_str().map(str::to_owned);
            if after.is_none() {
                break;
            }
        }
    }
    assert_eq!(all.len(), cells);
    // Report observed timings; no machine-specific speed is required for a pass.
    Ok(serde_json::json!({
        "kind": "real_files_synthetic_logical_participants",
        "participants": count, "cells": cells,
        "real_worker_processes": 0, "model_calls": 0,
        "registration_ms": registration_ms, "initial_poll_ms": initial_poll_ms,
        "context_reads_ms": initial_context_ms + delayed_context_ms,
        "elapsed_ms": started.elapsed().as_millis(),
        "first_pass_accepted": accepted, "max_scanned_per_poll": max_scanned,
        "context_reads": context_reads, "absent_consumers": count / 10,
        "delayed_consumers": count / 10,
        "max_context_bytes": max_context_bytes, "max_selected_messages": max_selected_messages,
        "noisy_additional_accepted": noisy_accepted, "noisy_rejected": noisy_rejected,
        "noisy_duplicates": noisy_duplicates, "noisy_cell_total": lifetime_total,
        "restart_preserved_quotas": true, "expired_context_empty": true,
        "stale_open_view_did_not_block_publication": true,
        "cross_cell_identity_rejected": true, "paginated_cohorts": all.len(),
        "limit": "Local filesystem protocol only; no model quality, distributed availability or production federation claim."
    }))
}

#[test]
#[ignore = "real filesystem stress with 1000/10000 logical members; no model calls"]
fn thousand_and_ten_thousand_logical_members_respect_board_limits() -> Result<()> {
    for count in [1000, 10_000] {
        println!("{}", serde_json::to_string(&exercise(count)?)?);
    }
    Ok(())
}
