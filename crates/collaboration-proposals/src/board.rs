//! Host operations coordinate one authority commit before publishing derived views.
use crate::{
    admission::consume_slot,
    authority::{MemberRecord, advance_time, member_record, task_counts},
    files::private_directory,
    limits::{
        CapacityKind, MAX_COHORT_MESSAGES, MAX_KNOWN_COHORTS, MAX_MEMBERS, MAX_RETAINED_COHORTS,
        MAX_RUN_MESSAGES, MAX_TASK_MESSAGES, OUTBOX_SLOTS, POLL_SLOTS, TRUST, TTL_SECONDS,
        capacity,
    },
    presence::{self, Presence},
    projection::{context_snapshot, view_value},
    protocol::{ContextSnapshot, Endpoint, HostMember, PollReport, Selection, digest_id, topics},
    store::{IndexEntry, Store, shard_of},
};
use anyhow::{Context, Result, ensure};
use contracts::OriginState;
use std::{collections::BTreeMap, path::Path};

pub struct Board {
    pub(super) store: Store,
}

impl Board {
    /// Open existing host authority without creating directories or changing permissions.
    pub fn open(state_dir: &Path) -> Result<Self> {
        Ok(Self {
            store: Store::open(state_dir)?,
        })
    }

    pub fn new(state_dir: &Path) -> Result<Self> {
        Ok(Self {
            store: Store::new(state_dir)?,
        })
    }

    /// Explicit host opt-in. Registration replay preserves counters, slots and TTL.
    pub fn register(&self, member: &HostMember, now: u64) -> Result<Endpoint> {
        member.validate()?;
        let _lock = self.store.lock(&member.cohort_id)?;
        let shard = shard_of(&member.cohort_id)?;
        let mut index = self.store.index(shard)?;
        let new_cohort = !index.cohorts.contains_key(&member.cohort_id);
        if let Some(entry) = index.cohorts.get(&member.cohort_id) {
            ensure!(!entry.pruned, "cohort identity is permanently retired");
        } else {
            capacity(
                index.cohorts.len() < MAX_KNOWN_COHORTS,
                CapacityKind::KnownCohorts,
            )?;
            capacity(
                index.cohorts.values().filter(|e| !e.pruned).count() < MAX_RETAINED_COHORTS,
                CapacityKind::RetainedCohorts,
            )?;
            index.cohorts.insert(
                member.cohort_id.clone(),
                IndexEntry {
                    created_at: now,
                    initialized: false,
                    pruned: false,
                },
            );
        }
        let mut c = self
            .store
            .registration_state(&member.cohort_id, &index, now)?;
        ensure!(c.closed_at.is_none(), "cohort is closed");
        advance_time(&mut c, now)?;
        if let Some(existing) = c
            .members
            .iter()
            .find(|m| m.identity.run_id == member.run_id)
        {
            ensure!(
                existing.identity == *member,
                "run registration authority changed"
            );
            self.store.reserve_run(member, true)?;
        } else {
            capacity(c.members.len() < MAX_MEMBERS, CapacityKind::Members)?;
            self.store.reserve_run(member, false)?;
            c.members.push(MemberRecord {
                identity: member.clone(),
                status: OriginState::Active,
                registered_at: now,
                accepted: 0,
                slots: BTreeMap::new(),
                inbox: Some(presence::Inbox::new(now)),
            });
            c.revision = c
                .revision
                .checked_add(1)
                .context("board revision overflow")?;
        }
        let endpoint = self.store.endpoint(member);
        if new_cohort {
            self.store.write_index(shard, &index)?;
        }
        private_directory(&endpoint.outbox)?;
        private_directory(&endpoint.view)?;
        presence::flush(&mut c, now);
        self.store.save(&c)?;
        if !index.cohorts[&member.cohort_id].initialized {
            index
                .cohorts
                .get_mut(&member.cohort_id)
                .unwrap()
                .initialized = true;
            self.store.write_index(shard, &index)?;
        }
        self.store.write_view(&c, now)?;
        Ok(endpoint)
    }

    /// At most 32 fixed-slot reads (each <=4097 bytes); no directory enumeration.
    pub fn poll(&self, id: &str, now: u64) -> Result<PollReport> {
        self.poll_committing(id, now, || Ok(()))
    }

    pub(super) fn poll_committing(
        &self,
        id: &str,
        now: u64,
        after_commit: impl FnOnce() -> Result<()>,
    ) -> Result<PollReport> {
        self.poll_observing(id, &[], now, after_commit)
    }

    /// The runtime supplies at most eight live host-bound members; their public
    /// session activity controls inbox delivery before new proposals are admitted.
    pub fn poll_live(&self, id: &str, live: &[HostMember], now: u64) -> Result<PollReport> {
        ensure!(live.len() <= 8, "too many live presence observations");
        self.poll_observing(id, live, now, || Ok(()))
    }

    fn poll_observing(
        &self,
        id: &str,
        live: &[HostMember],
        now: u64,
        after_commit: impl FnOnce() -> Result<()>,
    ) -> Result<PollReport> {
        let _lock = self.store.lock(id)?;
        let mut c = self.store.load(id)?;
        advance_time(&mut c, now)?;
        presence::initialize(&mut c);
        for member in live {
            member.validate()?;
            member_record(&c, member)?;
            let record = c
                .members
                .iter_mut()
                .find(|m| m.identity == *member)
                .unwrap();
            presence::observe(self.store.state_dir(), record, now)?;
        }
        let mut report = PollReport {
            scanned_slots: 0,
            accepted: 0,
            duplicates: 0,
            rejected: 0,
            changed_consumed_slots: 0,
            cohort_accepted_total: 0,
            expired_messages: 0,
            cohort_capacity_exhausted: false,
            run_quota_exhausted: vec![],
            task_quota_exhausted: vec![],
            revision: c.revision,
        };
        let total = c.members.len() * OUTBOX_SLOTS;
        if c.closed_at.is_none() && total > 0 {
            for _ in 0..POLL_SLOTS.min(total) {
                let pos = c.cursor % total;
                c.cursor = (pos + 1) % total;
                // Visit one slot per member before moving to the next slot: a
                // 32-member cell can expose every member's first proposal in one poll.
                let mi = pos % c.members.len();
                let slot = pos / c.members.len();
                consume_slot(&self.store, &mut c, mi, slot, now, &mut report)?;
            }
        }
        c.revision = c
            .revision
            .checked_add(1)
            .context("board revision overflow")?;
        // This durable commit owns publication, dedup and quotas. View is only a cache.
        presence::flush(&mut c, now);
        self.store.save(&c)?;
        after_commit()?;
        self.store.write_view(&c, now)?;
        report.cohort_accepted_total = c.messages.len();
        report.expired_messages = c.messages.iter().filter(|m| m.expires_at <= now).count();
        report.cohort_capacity_exhausted = c.messages.len() == MAX_COHORT_MESSAGES;
        report.run_quota_exhausted = c
            .members
            .iter()
            .filter(|m| m.accepted >= MAX_RUN_MESSAGES)
            .map(|m| m.identity.run_id.clone())
            .collect();
        report.task_quota_exhausted = task_counts(&c)
            .into_iter()
            .filter(|(_, n)| *n >= MAX_TASK_MESSAGES)
            .map(|(id, _)| id)
            .collect();
        report.revision = c.revision;
        Ok(report)
    }

    /// Bounded public status/view reconstructed from authority, never from worker files.
    pub fn view(&self, id: &str, now: u64) -> Result<serde_json::Value> {
        let c = self.store.load(id)?;
        ensure!(now >= c.last_time, "host time moved backwards");
        view_value(&c, now, None)
    }

    /// Host-authorized worker view; the same filter applies to exported inboxes
    /// and frozen context. Reading never grants membership or acknowledges delivery.
    pub fn inbox(&self, member: &HostMember, now: u64) -> Result<serde_json::Value> {
        member.validate()?;
        let c = self.store.load(&member.cohort_id)?;
        member_record(&c, member)?;
        ensure!(now >= c.last_time, "host time moved backwards");
        view_value(&c, now, Some(member))
    }

    /// Only a host-bound live run may change availability. A terminal run needs
    /// a newly registered retry to reconnect; transport input cannot set presence.
    pub fn set_presence(&self, member: &HostMember, state: Presence, now: u64) -> Result<()> {
        member.validate()?;
        let _lock = self.store.lock(&member.cohort_id)?;
        let mut c = self.store.load(&member.cohort_id)?;
        member_record(&c, member)?;
        advance_time(&mut c, now)?;
        presence::initialize(&mut c);
        let record = c
            .members
            .iter_mut()
            .find(|m| m.identity == *member)
            .unwrap();
        presence::set(record, state, now)?;
        presence::flush(&mut c, now);
        c.revision = c
            .revision
            .checked_add(1)
            .context("board revision overflow")?;
        self.store.save(&c)?;
        self.store.write_view(&c, now)
    }

    /// Observer-only retained history; expired/revoked proposals never become context.
    pub fn inspect(&self, id: &str, now: u64) -> Result<serde_json::Value> {
        let c = self.store.load(id)?;
        ensure!(now >= c.last_time, "host time moved backwards");
        let mut value = view_value(&c, now, None)?;
        value["retained_messages"] = serde_json::Value::Array(c.messages.iter().enumerate().map(|(index,m)| {
            let origin = &c.members.iter().find(|member| member.identity.run_id == m.run_id).unwrap().status;
            serde_json::json!({"message":m,"origin":origin,"expired":m.expires_at <= now,"delivery":presence::dispatch(&c,index,now),"trust":TRUST})
        }).collect());
        Ok(value)
    }

    /// At most one shard and 64 rows. The caller advances next_after, then next_shard.
    pub fn cohorts(
        &self,
        shard: u8,
        after: Option<&str>,
        limit: usize,
    ) -> Result<serde_json::Value> {
        ensure!((1..=64).contains(&limit), "cohort page limit must be 1..64");
        if let Some(id) = after {
            ensure!(shard_of(id)? == shard, "cursor belongs to another shard");
        }
        let index = self.store.index(shard)?;
        let mut rows = index
            .cohorts
            .iter()
            .filter(|(id, _)| after.is_none_or(|a| id.as_str() > a));
        let page: Vec<_> = rows.by_ref().take(limit).map(|(id,entry)|serde_json::json!({"cohort_id":id,"created_at":entry.created_at,"initialized":entry.initialized,"pruned":entry.pruned})).collect();
        let more = rows.next().is_some();
        let next_after = more.then(|| page.last().unwrap()["cohort_id"].as_str().unwrap());
        Ok(
            serde_json::json!({"schema_version":1,"shard":shard,"cohorts":page,"next_after":next_after,"next_shard":if more { None } else { shard.checked_add(1) }}),
        )
    }

    /// Context selection cannot confer membership and does not consume/ack messages.
    pub fn context(
        &self,
        member: &HostMember,
        selection: &Selection,
        now: u64,
    ) -> Result<ContextSnapshot> {
        member.validate()?;
        topics(&selection.topics, 8)?;
        ensure!(selection.query.len() <= 256, "context query too long");
        let c = self.store.load(&member.cohort_id)?;
        member_record(&c, member)?;
        ensure!(now >= c.last_time, "host time moved backwards");
        context_snapshot(&c, member, selection, now)
    }

    pub fn complete_run(&self, member: &HostMember, receipt_sha256: &str, now: u64) -> Result<()> {
        digest_id(receipt_sha256)?;
        self.set_origin(
            member,
            OriginState::Completed {
                receipt_sha256: receipt_sha256.into(),
            },
            now,
        )
    }

    pub fn revoke_run(&self, member: &HostMember, reason: &str, now: u64) -> Result<()> {
        ensure!(
            !reason.trim().is_empty() && reason.len() <= 256,
            "invalid host revocation reason"
        );
        self.set_origin(
            member,
            OriginState::Revoked {
                reason: reason.into(),
            },
            now,
        )
    }

    fn set_origin(&self, member: &HostMember, status: OriginState, now: u64) -> Result<()> {
        member.validate()?;
        let _lock = self.store.lock(&member.cohort_id)?;
        let mut c = self.store.load(&member.cohort_id)?;
        member_record(&c, member)?;
        let mi = c
            .members
            .iter()
            .position(|m| m.identity.run_id == member.run_id)
            .unwrap();
        if c.members[mi].status == status {
            // Repair only a lost/stale export after commit. Historical terminal
            // replay neither increments revision nor rewrites an intact export.
            let intact = self.store.exports_intact(&c);
            if !intact {
                self.store.write_view(&c, c.last_time)?;
            }
            return Ok(());
        }
        ensure!(
            c.members[mi].status == OriginState::Active,
            "terminal origin state cannot change"
        );
        advance_time(&mut c, now)?;
        presence::initialize(&mut c);
        if matches!(status, OriginState::Completed { .. }) {
            // The worker has exited and workflow success is host-confirmed.
            // Consume its last proposals in this same terminal state commit.
            let mut report = PollReport::default();
            for slot in 0..OUTBOX_SLOTS {
                consume_slot(&self.store, &mut c, mi, slot, now, &mut report)?;
            }
        }
        presence::set(&mut c.members[mi], Presence::Offline, now)?;
        c.members[mi].status = status;
        presence::flush(&mut c, now);
        c.revision = c
            .revision
            .checked_add(1)
            .context("board revision overflow")?;
        self.store.save(&c)?;
        self.store.write_view(&c, now)
    }

    pub fn close_cohort(&self, id: &str, now: u64) -> Result<()> {
        let _lock = self.store.lock(id)?;
        let mut c = self.store.load(id)?;
        advance_time(&mut c, now)?;
        ensure!(
            c.members.iter().all(|m| m.status != OriginState::Active),
            "active cohort cannot be closed"
        );
        if c.closed_at.is_none() {
            c.closed_at = Some(now);
            c.revision = c
                .revision
                .checked_add(1)
                .context("board revision overflow")?;
        }
        self.store.save(&c)?;
        self.store.write_view(&c, now)
    }

    /// Explicit host authorization only, after any required evidence archiving.
    /// Retains a bounded tombstone: the same cohort can never reset its quotas.
    pub fn prune(&self, id: &str, now: u64) -> Result<()> {
        digest_id(id)?;
        let _lock = self.store.lock(id)?;
        let shard = shard_of(id)?;
        let mut index = self.store.index(shard)?;
        let entry = index.cohorts.get(id).context("unknown cohort")?;
        if !entry.pruned {
            let c = self.store.load(id)?;
            let closed = c.closed_at.context("host has not closed this cohort")?;
            ensure!(
                now >= closed.saturating_add(TTL_SECONDS)
                    && c.messages.iter().all(|m| m.expires_at <= now),
                "cohort has not expired"
            );
            index.cohorts.get_mut(id).unwrap().pruned = true;
            self.store.write_index(shard, &index)?;
        }
        self.store.prune_files(id, shard)
    }
}
