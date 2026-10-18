use super::*;
fn proposal(id: u64) -> Proposal {
    Proposal {
        id,
        mission: 1,
        producer: 0,
        topic: 0,
        reply_to: None,
    }
}

#[test]
fn rejects_invalid_population() {
    assert!(simulate(0).is_err());
    assert!(simulate(10_001).is_err());
}

#[test]
fn admission_identity_depth_ttl_and_mission_are_enforced() {
    let mut f = Fabric::new(32, 1).unwrap();
    assert_eq!(f.admit(proposal(1), 0), Admission::Accepted);
    assert_eq!(f.admit(proposal(1), 0), Admission::Duplicate);
    let mut changed = proposal(1);
    changed.topic = 3;
    assert_eq!(f.admit(changed, 0), Admission::Conflict);
    for (id, parent) in [(2, 1), (3, 2)] {
        let mut p = proposal(id);
        p.reply_to = Some((0, 0, parent));
        assert_eq!(f.admit(p, 0), Admission::Accepted);
    }
    let mut deep = proposal(4);
    deep.reply_to = Some((0, 0, 3));
    assert_eq!(f.admit(deep, 0), Admission::ReplyDepth);
    let mut foreign = proposal(5);
    foreign.mission = 2;
    assert_eq!(f.admit(foreign, 0), Admission::CrossMission);
    assert!(f.pull(2, 0, 0).is_err());
    f.tick(TTL);
    assert_eq!(f.measurements.expired_pending, 3);
    assert_eq!(f.dedup_entries, 0);
    let mut stale = proposal(6);
    stale.reply_to = Some((0, 0, 1));
    assert_eq!(f.admit(stale, TTL), Admission::ReplyUnavailable);
    assert_eq!(f.admit(proposal(1), TTL), Admission::Accepted);
}

#[test]
fn absent_reader_cannot_block_exports_and_slow_cursor_reports_loss() {
    let mut f = Fabric::new(32, 1).unwrap();
    for tick in 0..48 {
        f.tick(tick);
        assert_eq!(f.admit(proposal(tick), tick), Admission::Accepted);
        f.export(tick).unwrap();
    }
    assert_eq!(f.measurements.exported, 48);
    assert_eq!(f.pending, 0);
    assert_eq!(
        f.index[&(FEDERATED, 0)].records.len(),
        FEDERATED_TOPIC_LIMIT
    );
    assert_eq!(f.pull(1, 0, 48).unwrap().len(), PAGE_SIZE);
    assert_eq!(f.measurements.observed_gaps, 1);
    assert_eq!(f.members[0].cursors[0], 18);
}

#[test]
fn thousand_and_ten_thousand_noisy_participants_have_bounded_pull_and_fair_export() {
    for count in [1_000, 10_000] {
        let r = simulate(count).unwrap();
        let cells = count.div_ceil(CELL_SIZE);
        let m = &r["observed"];
        assert!(m["queue_full"].as_u64().unwrap() > 0);
        assert!(m["duplicates"].as_u64().unwrap() > 0);
        assert!(m["observed_gaps"].as_u64().unwrap() > 0);
        assert_eq!(m["admitted"], m["exported"]);
        assert_eq!(m["expired_pending"], 0);
        assert!(m["peak_pending"].as_u64().unwrap() <= (cells * QUEUE_LIMIT) as u64);
        assert!(m["peak_dedup_entries"].as_u64().unwrap() <= (cells * DEDUP_LIMIT) as u64);
        assert!(
            m["peak_index_records"].as_u64().unwrap()
                <= r["limits"]["index_records_total"].as_u64().unwrap()
        );
        assert!(m["max_exported_per_tick"].as_u64().unwrap() <= GLOBAL_EXPORT_LIMIT as u64);
        assert!(m["max_cell_exported_per_tick"].as_u64().unwrap() <= CELL_EXPORT_LIMIT as u64);
        assert!(
            m["max_export_metadata_bytes_per_tick"].as_u64().unwrap()
                <= (GLOBAL_EXPORT_LIMIT * MAX_SUMMARY_BYTES) as u64
        );
        assert!(
            m["max_selected_per_pull"].as_u64().unwrap() <= (PAGE_SIZE * PAGES_PER_PULL) as u64
        );
        assert!(m["max_context_bytes"].as_u64().unwrap() <= MAX_CONTEXT_BYTES as u64);
        assert!(
            m["queue_visits"].as_u64().unwrap()
                <= r["ticks"].as_u64().unwrap() * (cells * CELL_EXPORT_LIMIT) as u64
        );
        assert_eq!(r["actual_workers_started"], 0);
        assert_eq!(r["model_requests"], 0);
    }
}
