//! A bounded, single-process protocol simulation, never a distributed/LLM benchmark.
//! Cells export only host-admitted metadata. Readers pull; publication never wakes
//! an agent, schedules work, changes permissions, or publishes to SuperPOD.
use anyhow::{Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

const CELL_SIZE: usize = 32;
const TOPICS: usize = 16;
const SUBSCRIPTIONS: usize = 4;
const MAX_SUBSCRIPTIONS: usize = 8;
const QUEUE_LIMIT: usize = 8;
const CELL_EXPORT_LIMIT: usize = 2;
const BUCKET_CAPACITY: usize = 2;
const BUCKET_REFILL: usize = 1;
const GLOBAL_EXPORT_LIMIT: usize = 64;
const TTL: u64 = 64;
const DEDUP_LIMIT: usize = QUEUE_LIMIT + CELL_EXPORT_LIMIT * TTL as usize;
const LOCAL_TOPIC_LIMIT: usize = 16;
const FEDERATED_TOPIC_LIMIT: usize = 32;
const PAGE_SIZE: usize = 2;
const PAGES_PER_PULL: usize = 2;
const MAX_CONTEXT_BYTES: usize = 2048;
const MAX_SUMMARY_BYTES: usize = 384;
const MAX_REPLY_DEPTH: u8 = 2;
const FEDERATED: usize = usize::MAX;
const PRODUCER_TICKS: u64 = 64;

#[derive(Clone)]
struct Member {
    cell: usize,
    topics: Vec<usize>,
    cursors: Vec<u64>,
    topic_cursor: usize,
}

/// No message body, instruction, tool output, credential or source text is copied.
#[derive(Clone, Serialize)]
struct Summary {
    id: u64,
    mission: u64,
    producer: usize,
    cell: usize,
    topic: usize,
    admitted_tick: u64,
    expires_tick: u64,
    reply_depth: u8,
}

struct Proposal {
    id: u64,
    mission: u64,
    producer: usize,
    topic: usize,
    reply_to: Option<(usize, usize, u64)>,
}

#[derive(Debug, PartialEq, Eq)]
enum Admission {
    Accepted,
    Duplicate,
    Conflict,
    CrossMission,
    InvalidMemberOrTopic,
    ReplyUnavailable,
    ReplyDepth,
    QueueFull,
    DedupFull,
}

struct Cell {
    queue: VecDeque<Summary>,
    // Host-allocated IDs bind producer and payload; changed bindings conflict.
    seen: HashMap<u64, Seen>,
    tokens: usize,
    exported_this_tick: usize,
    exports: usize,
}

struct Seen {
    producer: usize,
    topic: usize,
    reply_to: Option<(usize, usize, u64)>,
    expires: u64,
}

#[derive(Default)]
struct Stream {
    sequence: u64,
    records: VecDeque<(u64, Summary)>,
}

#[derive(Default, Serialize)]
struct Measurements {
    proposals_attempted: usize,
    admitted: usize,
    duplicates: usize,
    rejected: usize,
    queue_full: usize,
    exported: usize,
    expired_pending: usize,
    queue_visits: usize,
    index_lookups: usize,
    message_inspections: usize,
    pull_calls: usize,
    pages_read: usize,
    observed_gaps: usize,
    peak_pending: usize,
    peak_dedup_entries: usize,
    peak_index_records: usize,
    max_export_wait_ticks: u64,
    max_exported_per_tick: usize,
    max_cell_exported_per_tick: usize,
    max_export_metadata_bytes_per_tick: usize,
    deferred_queue_ticks: usize,
    max_selected_per_pull: usize,
    max_context_bytes: usize,
}

struct Fabric {
    mission: u64,
    members: Vec<Member>,
    cells: Vec<Cell>,
    // Direct cell/topic lookup: never inspect another mission or scan all cells
    // to find a topic. FEDERATED is a bounded metadata index, not a broadcast queue.
    index: HashMap<(usize, usize), Stream>,
    export_cursor: usize,
    pending: usize,
    dedup_entries: usize,
    index_records: usize,
    measurements: Measurements,
}

impl Fabric {
    fn new(count: usize, mission: u64) -> Result<Self> {
        ensure!(
            (1..=10_000).contains(&count),
            "agent_count must be 1..10000"
        );
        let members = (0..count)
            .map(|id| Member {
                cell: id / CELL_SIZE,
                topics: [0, 3, 7, 11].iter().map(|d| (id + d) % TOPICS).collect(),
                cursors: vec![0; SUBSCRIPTIONS],
                topic_cursor: 0,
            })
            .collect();
        let cells = (0..count.div_ceil(CELL_SIZE))
            .map(|_| Cell {
                queue: VecDeque::new(),
                seen: HashMap::new(),
                tokens: BUCKET_CAPACITY,
                exported_this_tick: 0,
                exports: 0,
            })
            .collect();
        Ok(Self {
            mission,
            members,
            cells,
            index: HashMap::new(),
            export_cursor: 0,
            pending: 0,
            dedup_entries: 0,
            index_records: 0,
            measurements: Measurements::default(),
        })
    }

    fn tick(&mut self, now: u64) {
        for cell in &mut self.cells {
            cell.tokens = (cell.tokens + BUCKET_REFILL).min(BUCKET_CAPACITY);
            cell.exported_this_tick = 0;
            let before = cell.seen.len();
            cell.seen.retain(|_, value| value.expires > now);
            self.dedup_entries -= before - cell.seen.len();
            while cell.queue.front().is_some_and(|m| m.expires_tick <= now) {
                cell.queue.pop_front();
                self.pending -= 1;
                self.measurements.expired_pending += 1;
            }
        }
        for stream in self.index.values_mut() {
            while stream
                .records
                .front()
                .is_some_and(|(_, m)| m.expires_tick <= now)
            {
                stream.records.pop_front();
                self.index_records -= 1;
            }
        }
    }

    fn admit(&mut self, proposal: Proposal, now: u64) -> Admission {
        self.measurements.proposals_attempted += 1;
        let outcome = self.admit_inner(proposal, now);
        match outcome {
            Admission::Accepted => self.measurements.admitted += 1,
            Admission::Duplicate => self.measurements.duplicates += 1,
            _ => {
                self.measurements.rejected += 1;
                self.measurements.queue_full += usize::from(outcome == Admission::QueueFull);
            }
        }
        outcome
    }

    fn admit_inner(&mut self, p: Proposal, now: u64) -> Admission {
        if p.mission != self.mission {
            return Admission::CrossMission;
        }
        let Some(member) = self.members.get(p.producer) else {
            return Admission::InvalidMemberOrTopic;
        };
        if !member.topics.contains(&p.topic) {
            return Admission::InvalidMemberOrTopic;
        }
        let cell_id = member.cell;
        if let Some(old) = self.cells[cell_id].seen.get(&p.id) {
            return if (old.producer, old.topic, old.reply_to) == (p.producer, p.topic, p.reply_to) {
                Admission::Duplicate
            } else {
                Admission::Conflict
            };
        }
        let depth = if let Some((cell, topic, id)) = p.reply_to {
            self.measurements.index_lookups += 1;
            let Some(stream) = self.index.get(&(cell, topic)) else {
                return Admission::ReplyUnavailable;
            };
            let mut found = None;
            for (_, message) in &stream.records {
                self.measurements.message_inspections += 1;
                if message.id == id && message.expires_tick > now {
                    found = Some(message.reply_depth);
                    break;
                }
            }
            let Some(parent_depth) = found else {
                return Admission::ReplyUnavailable;
            };
            if parent_depth >= MAX_REPLY_DEPTH {
                return Admission::ReplyDepth;
            }
            parent_depth + 1
        } else {
            0
        };
        let cell = &mut self.cells[cell_id];
        if cell.queue.len() >= QUEUE_LIMIT {
            return Admission::QueueFull;
        }
        if cell.seen.len() >= DEDUP_LIMIT {
            return Admission::DedupFull;
        }
        let message = Summary {
            id: p.id,
            mission: p.mission,
            producer: p.producer,
            cell: cell_id,
            topic: p.topic,
            admitted_tick: now,
            expires_tick: now + TTL,
            reply_depth: depth,
        };
        cell.seen.insert(
            p.id,
            Seen {
                producer: p.producer,
                topic: p.topic,
                reply_to: p.reply_to,
                expires: now + TTL,
            },
        );
        cell.queue.push_back(message.clone());
        self.pending += 1;
        self.dedup_entries += 1;
        self.measurements.peak_pending = self.measurements.peak_pending.max(self.pending);
        self.measurements.peak_dedup_entries =
            self.measurements.peak_dedup_entries.max(self.dedup_entries);
        self.index_message(cell_id, message, LOCAL_TOPIC_LIMIT);
        Admission::Accepted
    }

    fn index_message(&mut self, cell: usize, message: Summary, limit: usize) {
        self.measurements.index_lookups += 1;
        let stream = self.index.entry((cell, message.topic)).or_default();
        stream.sequence += 1;
        stream.records.push_back((stream.sequence, message));
        self.index_records += 1;
        if stream.records.len() > limit {
            stream.records.pop_front();
            self.index_records -= 1;
        }
        self.measurements.peak_index_records =
            self.measurements.peak_index_records.max(self.index_records);
    }

    fn export(&mut self, now: u64) -> Result<()> {
        let mut exported = 0;
        let mut metadata_bytes = 0;
        // Cursor survives tick boundaries. At most two complete sweeps, and no
        // per-reader fanout, even when all consumers are absent.
        for _ in 0..self.cells.len() * CELL_EXPORT_LIMIT {
            if exported == GLOBAL_EXPORT_LIMIT {
                break;
            }
            let id = self.export_cursor;
            self.export_cursor = (self.export_cursor + 1) % self.cells.len();
            self.measurements.queue_visits += 1;
            let cell = &mut self.cells[id];
            if cell.tokens == 0 || cell.exported_this_tick == CELL_EXPORT_LIMIT {
                continue;
            }
            if let Some(message) = cell.queue.pop_front() {
                let bytes = serde_json::to_vec(&message)?.len();
                ensure!(bytes <= MAX_SUMMARY_BYTES, "export metadata size exceeded");
                metadata_bytes += bytes;
                cell.tokens -= 1;
                cell.exported_this_tick += 1;
                cell.exports += 1;
                self.measurements.max_cell_exported_per_tick = self
                    .measurements
                    .max_cell_exported_per_tick
                    .max(cell.exported_this_tick);
                self.pending -= 1;
                exported += 1;
                self.measurements.max_export_wait_ticks = self
                    .measurements
                    .max_export_wait_ticks
                    .max(now - message.admitted_tick);
                self.index_message(FEDERATED, message, FEDERATED_TOPIC_LIMIT);
            }
        }
        self.measurements.exported += exported;
        self.measurements.max_exported_per_tick =
            self.measurements.max_exported_per_tick.max(exported);
        self.measurements.max_export_metadata_bytes_per_tick = self
            .measurements
            .max_export_metadata_bytes_per_tick
            .max(metadata_bytes);
        self.measurements.deferred_queue_ticks += self.pending;
        ensure!(
            metadata_bytes <= GLOBAL_EXPORT_LIMIT * MAX_SUMMARY_BYTES,
            "global metadata budget exceeded"
        );
        Ok(())
    }

    fn pull(&mut self, mission: u64, member_id: usize, now: u64) -> Result<Vec<Summary>> {
        ensure!(mission == self.mission, "cross-mission pull rejected");
        let member = self
            .members
            .get_mut(member_id)
            .ok_or_else(|| anyhow::anyhow!("unknown member"))?;
        ensure!(
            member.topics.len() <= MAX_SUBSCRIPTIONS,
            "subscription limit exceeded"
        );
        let mut selected = Vec::new();
        self.measurements.pull_calls += 1;
        for _ in 0..PAGES_PER_PULL {
            let subscription = member.topic_cursor;
            member.topic_cursor = (member.topic_cursor + 1) % member.topics.len();
            self.measurements.index_lookups += 1;
            self.measurements.pages_read += 1;
            if let Some(stream) = self.index.get(&(FEDERATED, member.topics[subscription])) {
                let cursor = &mut member.cursors[subscription];
                let earliest = stream
                    .records
                    .front()
                    .map_or(stream.sequence + 1, |(seq, _)| *seq);
                if *cursor + 1 < earliest {
                    self.measurements.observed_gaps += 1;
                    *cursor = earliest - 1;
                }
                let mut page_count = 0;
                for (seq, message) in &stream.records {
                    self.measurements.message_inspections += 1;
                    if *seq <= *cursor || message.expires_tick <= now {
                        continue;
                    }
                    // Fixed typed metadata makes the total byte ceiling auditable.
                    ensure!(
                        serde_json::to_vec(message)?.len() <= MAX_SUMMARY_BYTES,
                        "metadata size exceeded"
                    );
                    selected.push(message.clone());
                    *cursor = *seq;
                    page_count += 1;
                    if page_count == PAGE_SIZE {
                        break;
                    }
                }
            }
        }
        let bytes = serde_json::to_vec(&selected)?.len();
        ensure!(bytes <= MAX_CONTEXT_BYTES, "context byte limit exceeded");
        self.measurements.max_context_bytes = self.measurements.max_context_bytes.max(bytes);
        self.measurements.max_selected_per_pull =
            self.measurements.max_selected_per_pull.max(selected.len());
        Ok(selected)
    }
}

/// Simulate bounded hierarchical communication for logical participants only.
/// It creates no processes, model requests, network connections, files or tasks.
pub fn simulate(agent_count: usize) -> Result<Value> {
    let started = Instant::now();
    let mut fabric = Fabric::new(agent_count, 1)?;
    let cells = fabric.cells.len();
    // Queue service bound accounts for global admission and per-cell refill.
    let service_bound = (cells * QUEUE_LIMIT).div_ceil(GLOBAL_EXPORT_LIMIT)
        + cells.div_ceil(GLOBAL_EXPORT_LIMIT)
        + QUEUE_LIMIT
        + 2;
    ensure!(
        service_bound < TTL as usize,
        "admitted queue cannot meet TTL"
    );
    let total_ticks = PRODUCER_TICKS + service_bound as u64;
    let mut per_member_deliveries = vec![0usize; agent_count];
    for tick in 0..total_ticks {
        fabric.tick(tick);
        if tick < PRODUCER_TICKS {
            // Rotate producer order: overload rejection must not always favor
            // the same member in a cell. No implied entitlement to admission.
            for cell in 0..cells {
                let start = cell * CELL_SIZE;
                let count = (agent_count - start).min(CELL_SIZE);
                for offset in 0..count {
                    let producer = start + (offset + tick as usize) % count;
                    let id = (tick << 32) | producer as u64;
                    let topic = fabric.members[producer].topics[tick as usize % SUBSCRIPTIONS];
                    let proposal = || Proposal {
                        id,
                        mission: 1,
                        producer,
                        topic,
                        reply_to: None,
                    };
                    let admitted = fabric.admit(proposal(), tick);
                    if admitted == Admission::Accepted {
                        ensure!(
                            fabric.admit(proposal(), tick) == Admission::Duplicate,
                            "replay changed admission"
                        );
                    }
                }
            }
            // One noisy producer makes additional distinct proposals; the same
            // queue/global limits apply and no new consumer work is pushed.
            for extra in 0..32 {
                fabric.admit(
                    Proposal {
                        id: (1u64 << 63) | (tick << 8) | extra,
                        mission: 1,
                        producer: 0,
                        topic: 0,
                        reply_to: None,
                    },
                    tick,
                );
            }
        }
        fabric.export(tick)?;
        for (id, delivered) in per_member_deliveries.iter_mut().enumerate() {
            if id % 10 == 0 || (id % 10 == 1 && tick % 17 != 0) {
                continue;
            }
            *delivered += fabric.pull(1, id, tick)?.len();
        }
    }
    ensure!(
        fabric.pending == 0 && fabric.measurements.expired_pending == 0,
        "admitted messages starved"
    );
    ensure!(
        fabric.measurements.admitted == fabric.measurements.exported,
        "accepted messages lost"
    );
    ensure!(fabric.cells.iter().all(|c| c.exports > 0), "cell starved");
    ensure!(
        fabric.measurements.max_export_wait_ticks <= service_bound as u64,
        "service bound exceeded"
    );
    let foreign_publish = fabric.admit(
        Proposal {
            id: u64::MAX,
            mission: 2,
            producer: 0,
            topic: 0,
            reply_to: None,
        },
        total_ticks,
    );
    ensure!(
        foreign_publish == Admission::CrossMission && fabric.pull(2, 0, total_ticks).is_err(),
        "mission boundary failed"
    );
    let active_readers = per_member_deliveries
        .iter()
        .enumerate()
        .filter(|(id, _)| id % 10 != 0)
        .count();
    let readers_receiving = per_member_deliveries
        .iter()
        .filter(|count| **count > 0)
        .count();
    Ok(json!({
        "schema_version":1,"kind":"bounded_logical_federation_simulation","agent_count":agent_count,
        "cell_count":cells,"members_per_cell":CELL_SIZE,"topics":TOPICS,"subscriptions_per_member":SUBSCRIPTIONS,
        "ticks":total_ticks,"producer_ticks":PRODUCER_TICKS,"actual_workers_started":0,"model_requests":0,
        "production_worker_cap":8,"elapsed_millis":started.elapsed().as_millis(),
        "limits":{"max_subscriptions":MAX_SUBSCRIPTIONS,"queue_per_cell":QUEUE_LIMIT,"export_per_cell_tick":CELL_EXPORT_LIMIT,
            "bucket_capacity":BUCKET_CAPACITY,"bucket_refill_per_tick":BUCKET_REFILL,"global_summaries_per_tick":GLOBAL_EXPORT_LIMIT,
            "global_metadata_bytes_per_tick":GLOBAL_EXPORT_LIMIT*MAX_SUMMARY_BYTES,"dedup_per_cell":DEDUP_LIMIT,
            "local_records_per_cell_topic":LOCAL_TOPIC_LIMIT,"federated_records_per_topic":FEDERATED_TOPIC_LIMIT,
            "reply_depth":MAX_REPLY_DEPTH,"ttl_ticks":TTL,"pages_per_pull":PAGES_PER_PULL,"records_per_page":PAGE_SIZE,
            "context_bytes":MAX_CONTEXT_BYTES,"pending_total":cells*QUEUE_LIMIT,
            "index_records_total":cells*TOPICS*LOCAL_TOPIC_LIMIT+TOPICS*FEDERATED_TOPIC_LIMIT},
        "observed":fabric.measurements,
        "fairness":{"admitted_all_exported":true,"all_cells_served":true,"service_bound_ticks":service_bound,
            "active_readers":active_readers,"readers_receiving":readers_receiving,"remaining_pending":fabric.pending,
            "scope":"FIFO admitted summaries and round-robin cells. Overload rejects proposals; subscription/TTL gaps are explicit, not guaranteed delivery to every reader."},
        "cross_mission":{"publish_rejected":true,"pull_rejected":true},
        "limitations":["Single-process logical protocol only; not 10000 live models, distributed availability or research-quality evidence.",
            "Token buckets count metadata summaries, not LLM tokens. Actual worker concurrency and global model/wall-time admission budgets are separate.",
            "Only host-admitted metadata enters federation. No automatic wake, task scheduling, authority change or SuperPOD publication.",
            "Slow/absent readers cannot block producers; bounded retention can lose reader history and reports cursor gaps.",
            "In-memory cursors/quotas have no crash durability proof. Production sharded authority needs separate file/restart tests."]
    }))
}

#[cfg(test)]
mod tests {
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
}
