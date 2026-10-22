//! Pure transitions; the storage owner supplies membership and persists atomically.
use crate::*;
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
fn check(value: bool, code: TaskErrorCode) -> Result<()> {
    ensure!(value, "{code}");
    Ok(())
}
fn bounded(value: &str, max: usize) -> Result<()> {
    check(
        !value.trim().is_empty() && value.len() <= max,
        TaskErrorCode::Invalid,
    )
}
impl TaskBudget {
    pub fn validate(&self) -> Result<()> {
        check(
            (1..=128).contains(&self.max_nodes)
                && (1..=10).contains(&self.max_attempts)
                && (1..=10).contains(&self.max_revisions)
                && (30..=43200).contains(&self.active_seconds)
                && (10..=3600).contains(&self.execution_seconds),
            TaskErrorCode::Invalid,
        )
    }
}
impl OutcomeTask {
    pub fn new(input: TaskInput, now: u64) -> Result<Self> {
        id(&input.id)?;
        id(&input.group_id)?;
        bounded(&input.intent, 16000)?;
        input.budget.validate()?;
        Ok(Self {
            title: input.intent.chars().take(80).collect(),
            input,
            kind: TaskKind::Outcome,
            acceptance: String::new(),
            state: TaskState::Planning,
            paused_from: None,
            revision: 1,
            graph_revision: 0,
            revisions_used: 0,
            nodes_created: 0,
            coordinator_calls: 0,
            active_ms: 0,
            clock_ms: now,
            created_ms: now,
            coordinator_lease: None,
            nodes: vec![],
            retired_nodes: vec![],
            graph_history: vec![],
            coordinator_attempts: vec![],
            artifacts: vec![],
            deliveries: vec![],
            events: vec![],
            notes: vec![],
            question: String::new(),
            error: None,
            reply_to: None,
        })
    }
    pub fn active(&self) -> bool {
        matches!(self.state, TaskState::Planning | TaskState::Running)
    }
    pub fn ready(&self, node: &TaskNode) -> bool {
        self.state == TaskState::Running
            && node.state == TaskNodeState::Pending
            && node.attempt < self.input.budget.max_attempts
            && node.input.dependencies.iter().all(|id| {
                self.nodes.iter().any(|n| {
                    n.input.id == *id
                        && n.state == TaskNodeState::Approved
                        && n.artifact_id.is_some()
                })
            })
    }
    pub fn needs_coordinator(&self) -> bool {
        self.kind == TaskKind::Outcome
            && self.active()
            && self.coordinator_lease.is_none()
            && !self.nodes.iter().any(|n| n.state == TaskNodeState::Running)
            && (self.state == TaskState::Planning
                || self.nodes.iter().any(|n| {
                    matches!(
                        n.state,
                        TaskNodeState::Submitted | TaskNodeState::Failed | TaskNodeState::Stale
                    )
                })
                || self
                    .nodes
                    .iter()
                    .all(|n| n.state == TaskNodeState::Approved))
    }
    pub fn record(&mut self, kind: TaskEventKind, node_id: Option<String>, text: String, now: u64) {
        self.events.push(TaskEvent {
            sequence: self.events.len() as u64 + 1,
            kind,
            node_id,
            text,
            created_ms: now,
        });
    }
    fn pause(&mut self, code: TaskErrorCode, now: u64) {
        if self.state != TaskState::Paused {
            self.paused_from = Some(self.state);
        }
        self.state = TaskState::Paused;
        self.error = Some(code);
        self.invalidate_leases(now);
        self.record(TaskEventKind::Paused, None, code.to_string(), now);
    }
    fn invalidate_leases(&mut self, now: u64) {
        if let Some(a) = self
            .coordinator_attempts
            .last_mut()
            .filter(|a| a.finished_ms.is_none())
        {
            a.finished_ms = Some(now);
            a.error = Some(TaskErrorCode::StaleAttempt);
        }
        self.coordinator_lease = None;
        for node in &mut self.nodes {
            if node.state == TaskNodeState::Running {
                node.state = TaskNodeState::Failed;
                if let Some(a) = node.attempts.last_mut() {
                    a.finished_ms = Some(now);
                    a.error = Some(TaskErrorCode::StaleAttempt);
                }
            }
            node.lease = None;
        }
    }
    pub fn advance_clock(&mut self, now: u64) {
        if self.active() {
            self.active_ms = self
                .active_ms
                .saturating_add(now.saturating_sub(self.clock_ms));
        }
        self.clock_ms = now;
        if self.active() && self.active_ms >= self.input.budget.active_seconds * 1000 {
            self.pause(TaskErrorCode::Budget, now);
        }
        if self
            .coordinator_lease
            .as_ref()
            .is_some_and(|l| l.deadline_ms <= now)
        {
            self.fail_coordinator(TaskErrorCode::Unavailable, now);
        }
        let mut expired = vec![];
        for n in &mut self.nodes {
            if n.lease.as_ref().is_some_and(|l| l.deadline_ms <= now) {
                n.lease = None;
                n.state = TaskNodeState::Failed;
                if let Some(a) = n.attempts.last_mut() {
                    a.finished_ms = Some(now);
                    a.error = Some(TaskErrorCode::Unavailable);
                }
                expired.push(n.input.id.clone());
            }
        }
        for id in expired {
            self.record(TaskEventKind::Expired, Some(id), String::new(), now);
        }
        self.finish_question(now);
        if self.active() && self.coordinator_calls >= 64 && self.coordinator_lease.is_none() {
            self.pause(TaskErrorCode::Budget, now);
        }
    }
    fn fail_coordinator(&mut self, code: TaskErrorCode, now: u64) {
        if let Some(a) = self.coordinator_attempts.last_mut() {
            a.finished_ms = Some(now);
            a.error = Some(code);
        }
        self.coordinator_lease = None;
        let failures = self
            .coordinator_attempts
            .iter()
            .rev()
            .take_while(|a| a.error.is_some())
            .count();
        if code == TaskErrorCode::Unavailable && failures < self.input.budget.max_attempts as usize
        {
            self.record(TaskEventKind::Submitted, None, code.to_string(), now);
        } else {
            self.pause(code, now);
        }
    }
    fn finish_question(&mut self, now: u64) {
        if self.kind == TaskKind::Question
            && self.active()
            && !self.nodes.is_empty()
            && self
                .nodes
                .iter()
                .all(|n| matches!(n.state, TaskNodeState::Approved | TaskNodeState::Failed))
        {
            if self.nodes.iter().any(|n| n.state == TaskNodeState::Failed) {
                self.pause(TaskErrorCode::Unavailable, now);
            } else {
                self.state = TaskState::Completed;
            }
        }
    }
    fn resume_question(&mut self) -> Result<()> {
        if self.kind == TaskKind::Question {
            check(
                self.nodes
                    .iter()
                    .filter(|n| n.state == TaskNodeState::Failed)
                    .all(|n| n.attempt < self.input.budget.max_attempts),
                TaskErrorCode::Budget,
            )?;
            for n in &mut self.nodes {
                if n.state == TaskNodeState::Failed {
                    n.state = TaskNodeState::Pending;
                }
            }
            self.state = TaskState::Running;
        }
        Ok(())
    }
    fn lease(&self, node: Option<&str>, claim: &str, now: u64) -> Result<()> {
        let lease = match node {
            Some(id) => self
                .nodes
                .iter()
                .find(|n| n.input.id == id)
                .and_then(|n| n.lease.as_ref()),
            None => self.coordinator_lease.as_ref(),
        };
        check(
            self.active()
                && lease.is_some_and(|l| {
                    l.claim_id == claim
                        && l.graph_revision == self.graph_revision
                        && l.deadline_ms > now
                }),
            TaskErrorCode::StaleAttempt,
        )
    }
    pub fn transition(
        &mut self,
        action: TaskAction,
        members: &BTreeSet<String>,
        now: u64,
    ) -> Result<()> {
        self.advance_clock(now);
        check(
            !matches!(self.state, TaskState::Completed | TaskState::Cancelled)
                || matches!(action, TaskAction::Tick),
            TaskErrorCode::Conflict,
        )?;
        match action {
            TaskAction::Tick => {}
            TaskAction::Resume => {
                check(
                    self.state == TaskState::Paused
                        && self.active_ms < self.input.budget.active_seconds * 1000,
                    TaskErrorCode::Budget,
                )?;
                self.state = self.paused_from.take().unwrap_or(TaskState::Planning);
                self.resume_question()?;
                self.error = None;
                self.record(TaskEventKind::Supplemented, None, String::new(), now);
            }
            TaskAction::Cancel => {
                self.state = TaskState::Cancelled;
                self.invalidate_leases(now);
                self.record(TaskEventKind::Cancelled, None, String::new(), now);
            }
            TaskAction::Supplement { text } => {
                check(self.kind == TaskKind::Outcome, TaskErrorCode::Conflict)?;
                bounded(&text, 8000)?;
                check(self.notes.len() < 64, TaskErrorCode::Budget)?;
                self.notes.push(text.clone());
                self.invalidate_leases(now);
                self.state = TaskState::Planning;
                self.error = None;
                self.question.clear();
                self.record(TaskEventKind::Supplemented, None, text, now);
            }
            TaskAction::Budget { budget } => {
                budget.validate()?;
                check(
                    budget.max_nodes >= self.nodes_created
                        && budget.active_seconds * 1000 > self.active_ms
                        && self.nodes.iter().all(|n| n.attempt <= budget.max_attempts),
                    TaskErrorCode::Budget,
                )?;
                self.input.budget = budget;
                self.error = None;
                if self.state == TaskState::Paused {
                    self.state = self.paused_from.take().unwrap_or(TaskState::Planning);
                    self.resume_question()?;
                }
                self.record(TaskEventKind::BudgetChanged, None, String::new(), now);
            }
            TaskAction::Accept => {
                check(self.state == TaskState::Review, TaskErrorCode::Conflict)?;
                self.deliveries
                    .last_mut()
                    .ok_or(TaskErrorCode::Invalid.to_string())
                    .map_err(anyhow::Error::msg)?
                    .accepted_ms = Some(now);
                self.state = TaskState::Completed;
                self.record(TaskEventKind::Accepted, None, String::new(), now);
            }
            TaskAction::Return { reason } => {
                bounded(&reason, 8000)?;
                check(self.state == TaskState::Review, TaskErrorCode::Conflict)?;
                self.deliveries.last_mut().unwrap().return_reason = Some(reason.clone());
                self.notes.push(reason.clone());
                self.state = TaskState::Planning;
                self.record(TaskEventKind::Returned, None, reason, now);
            }
            TaskAction::Pause { code } => self.pause(code, now),
            TaskAction::Claim { node_id, claim_id } => {
                id(&claim_id)?;
                check(self.active(), TaskErrorCode::Conflict)?;
                let lease = TaskLease {
                    claim_id: claim_id.clone(),
                    graph_revision: self.graph_revision,
                    deadline_ms: now
                        + (self.input.budget.active_seconds * 1000 - self.active_ms)
                            .min((self.input.budget.execution_seconds + 20) * 1000),
                };
                if let Some(id) = &node_id {
                    let index = self
                        .nodes
                        .iter()
                        .position(|n| n.input.id == *id)
                        .ok_or_else(|| anyhow::anyhow!("{}", TaskErrorCode::NotFound))?;
                    check(
                        self.coordinator_lease.is_none()
                            && self.ready(&self.nodes[index])
                            && members.contains(&self.nodes[index].input.person_id),
                        TaskErrorCode::Conflict,
                    )?;
                    let n = &mut self.nodes[index];
                    n.state = TaskNodeState::Running;
                    n.attempt += 1;
                    n.lease = Some(lease);
                    n.attempts.push(TaskAttempt {
                        attempt: n.attempt,
                        claim_id,
                        started_ms: now,
                        finished_ms: None,
                        error: None,
                        receipt: serde_json::Value::Null,
                    });
                } else {
                    check(self.needs_coordinator(), TaskErrorCode::Conflict)?;
                    check(self.coordinator_calls < 64, TaskErrorCode::Budget)?;
                    self.coordinator_calls += 1;
                    self.coordinator_attempts.push(TaskAttempt {
                        attempt: self.coordinator_calls,
                        claim_id: claim_id.clone(),
                        started_ms: now,
                        finished_ms: None,
                        error: None,
                        receipt: serde_json::Value::Null,
                    });
                    self.coordinator_lease = Some(lease);
                }
                self.record(TaskEventKind::Claimed, node_id, String::new(), now);
            }
            TaskAction::Fail {
                node_id,
                claim_id,
                code,
            } => {
                self.lease(node_id.as_deref(), &claim_id, now)?;
                if let Some(id) = node_id {
                    let n = self.nodes.iter_mut().find(|n| n.input.id == id).unwrap();
                    n.state = TaskNodeState::Failed;
                    n.lease = None;
                    let a = n.attempts.last_mut().unwrap();
                    a.finished_ms = Some(now);
                    a.error = Some(code);
                    self.record(TaskEventKind::Submitted, Some(id), code.to_string(), now);
                } else {
                    self.fail_coordinator(code, now);
                }
            }
            TaskAction::Submit {
                node_id,
                claim_id,
                summary,
                evidence,
                receipt,
                succeeded,
            } => {
                self.lease(Some(&node_id), &claim_id, now)?;
                bounded(&summary, 16000)?;
                check(
                    serde_json::to_vec(&evidence)?.len() <= 65536
                        && serde_json::to_vec(&receipt)?.len() <= 65536,
                    TaskErrorCode::Invalid,
                )?;
                let index = self
                    .nodes
                    .iter()
                    .position(|n| n.input.id == node_id)
                    .unwrap();
                let inputs = self.nodes[index]
                    .input
                    .dependencies
                    .iter()
                    .map(|id| {
                        self.nodes
                            .iter()
                            .find(|n| n.input.id == *id)
                            .and_then(|n| n.artifact_id.clone())
                            .ok_or_else(|| anyhow::anyhow!("{}", TaskErrorCode::Conflict))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let n = &mut self.nodes[index];
                check(
                    members.contains(&n.input.person_id),
                    TaskErrorCode::Forbidden,
                )?;
                n.lease = None;
                n.state = if succeeded {
                    TaskNodeState::Submitted
                } else {
                    TaskNodeState::Failed
                };
                let a = n.attempts.last_mut().unwrap();
                a.finished_ms = Some(now);
                a.receipt = receipt;
                if !succeeded {
                    a.error = Some(TaskErrorCode::Unavailable);
                }
                if succeeded {
                    let digest = crate::digest(&serde_json::to_vec(&(
                        &self.input.id,
                        &node_id,
                        n.attempt,
                        self.graph_revision,
                        &summary,
                        &evidence,
                        &inputs,
                    ))?);
                    let artifact = TaskArtifact {
                        id: format!("artifact-{digest}"),
                        node_id: node_id.clone(),
                        attempt: n.attempt,
                        graph_revision: self.graph_revision,
                        author_id: n.input.person_id.clone(),
                        summary,
                        evidence,
                        inputs,
                        digest,
                        created_ms: now,
                    };
                    n.artifact_id = Some(artifact.id.clone());
                    self.artifacts.push(artifact);
                    if self.kind == TaskKind::Question {
                        n.state = TaskNodeState::Approved;
                    }
                }
                self.record(TaskEventKind::Submitted, Some(node_id), String::new(), now);
            }
            TaskAction::Coordinate {
                claim_id,
                output,
                receipt,
            } => {
                self.lease(None, &claim_id, now)?;
                check(
                    output.protocol_version == COORDINATOR_PROTOCOL_VERSION,
                    TaskErrorCode::Protocol,
                )?;
                check(
                    serde_json::to_vec(&receipt)?.len() <= 65536,
                    TaskErrorCode::Invalid,
                )?;
                self.apply_proposal(output.proposal, members, now)?;
                if let Some(a) = self.coordinator_attempts.last_mut() {
                    a.finished_ms = Some(now);
                    a.receipt = receipt;
                }
                self.coordinator_lease = None;
            }
        }
        self.finish_question(now);
        self.revision += 1;
        check(
            self.events.len() <= 2048 && serde_json::to_vec(self)?.len() <= 4 * 1024 * 1024,
            TaskErrorCode::Budget,
        )
    }
    fn apply_proposal(
        &mut self,
        proposal: CoordinatorProposal,
        members: &BTreeSet<String>,
        now: u64,
    ) -> Result<()> {
        match proposal {
            CoordinatorProposal::Clarify { question } => {
                bounded(&question, 8000)?;
                self.question = question.clone();
                self.state = TaskState::WaitingInput;
                self.record(TaskEventKind::Clarification, None, question, now);
            }
            CoordinatorProposal::Plan {
                title,
                acceptance,
                nodes,
            } => {
                check(self.nodes.is_empty(), TaskErrorCode::Conflict)?;
                bounded(&title, 256)?;
                bounded(&acceptance, 8000)?;
                self.replace_graph(nodes, members, now)?;
                self.title = title;
                self.acceptance = acceptance;
                self.record(TaskEventKind::Planned, None, String::new(), now);
            }
            CoordinatorProposal::Revise { reason, nodes } => {
                bounded(&reason, 8000)?;
                check(
                    !self.nodes.is_empty() && self.revisions_used < self.input.budget.max_revisions,
                    TaskErrorCode::Budget,
                )?;
                self.replace_graph(nodes, members, now)?;
                self.revisions_used += 1;
                self.record(TaskEventKind::Revised, None, reason, now);
            }
            CoordinatorProposal::Check { reviews } => {
                check(self.state == TaskState::Running, TaskErrorCode::Conflict)?;
                check(
                    !reviews.is_empty() && reviews.len() <= self.nodes.len(),
                    TaskErrorCode::Invalid,
                )?;
                let mut seen = BTreeSet::new();
                for review in reviews {
                    bounded(&review.reason, 4000)?;
                    check(seen.insert(review.node_id.clone()), TaskErrorCode::Invalid)?;
                    let n = self
                        .nodes
                        .iter_mut()
                        .find(|n| n.input.id == review.node_id)
                        .ok_or_else(|| anyhow::anyhow!("{}", TaskErrorCode::NotFound))?;
                    check(n.state == TaskNodeState::Submitted, TaskErrorCode::Conflict)?;
                    n.state = if review.approved {
                        TaskNodeState::Approved
                    } else {
                        TaskNodeState::Failed
                    };
                    n.review = review.reason.clone();
                    self.record(
                        TaskEventKind::Checked,
                        Some(review.node_id),
                        review.reason,
                        now,
                    );
                }
            }
            CoordinatorProposal::Deliver { summary, node_ids } => {
                bounded(&summary, 16000)?;
                check(
                    self.state == TaskState::Running
                        && !self.nodes.is_empty()
                        && self
                            .nodes
                            .iter()
                            .all(|n| n.state == TaskNodeState::Approved)
                        && !node_ids.is_empty(),
                    TaskErrorCode::Conflict,
                )?;
                let mut seen = BTreeSet::new();
                let mut artifact_ids = vec![];
                for id in node_ids {
                    check(seen.insert(id.clone()), TaskErrorCode::Invalid)?;
                    artifact_ids.push(
                        self.nodes
                            .iter()
                            .find(|n| n.input.id == id)
                            .and_then(|n| n.artifact_id.clone())
                            .ok_or_else(|| anyhow::anyhow!("{}", TaskErrorCode::Invalid))?,
                    );
                }
                self.deliveries.push(TaskDelivery {
                    summary,
                    artifact_ids,
                    created_ms: now,
                    accepted_ms: None,
                    return_reason: None,
                });
                self.state = TaskState::Review;
                self.record(TaskEventKind::Delivered, None, String::new(), now);
            }
        }
        Ok(())
    }
    pub fn replace_graph(
        &mut self,
        inputs: Vec<TaskNodeInput>,
        members: &BTreeSet<String>,
        _now: u64,
    ) -> Result<()> {
        validate_graph(&inputs, members)?;
        let old: BTreeMap<_, _> = self
            .retired_nodes
            .iter()
            .chain(self.nodes.iter())
            .map(|n| (n.input.id.clone(), n.clone()))
            .collect();
        let added = inputs.iter().filter(|n| !old.contains_key(&n.id)).count() as u32;
        check(
            self.nodes_created + added <= self.input.budget.max_nodes,
            TaskErrorCode::Budget,
        )?;
        let mut invalid: BTreeSet<_> = inputs
            .iter()
            .filter(|n| {
                old.get(&n.id)
                    .is_none_or(|o| o.input != **n || o.state != TaskNodeState::Approved)
            })
            .map(|n| n.id.clone())
            .collect();
        loop {
            let before = invalid.len();
            for n in &inputs {
                if n.dependencies.iter().any(|d| invalid.contains(d)) {
                    invalid.insert(n.id.clone());
                }
            }
            if before == invalid.len() {
                break;
            }
        }
        let mut nodes = vec![];
        for input in inputs {
            let mut node = old.get(&input.id).cloned().unwrap_or(TaskNode {
                input: input.clone(),
                state: TaskNodeState::Pending,
                attempt: 0,
                lease: None,
                artifact_id: None,
                review: String::new(),
                attempts: vec![],
            });
            if invalid.contains(&input.id) {
                check(
                    node.attempt < self.input.budget.max_attempts,
                    TaskErrorCode::Budget,
                )?;
                node.state = TaskNodeState::Pending;
                node.artifact_id = None;
                node.review.clear();
            }
            node.input = input;
            node.lease = None;
            nodes.push(node);
        }
        let retained: BTreeSet<_> = nodes.iter().map(|n| n.input.id.clone()).collect();
        self.retired_nodes = old
            .into_values()
            .filter(|n| !retained.contains(&n.input.id))
            .collect();
        self.graph_history
            .push(nodes.iter().map(|n| n.input.clone()).collect());
        self.nodes = nodes;
        self.nodes_created += added;
        self.graph_revision += 1;
        self.state = TaskState::Running;
        Ok(())
    }
}
pub fn validate_graph(nodes: &[TaskNodeInput], members: &BTreeSet<String>) -> Result<()> {
    check(
        !nodes.is_empty() && nodes.len() <= 128,
        TaskErrorCode::Invalid,
    )?;
    let mut ids = BTreeSet::new();
    for n in nodes {
        id(&n.id)?;
        id(&n.person_id)?;
        check(
            n.person_id != "operator" && members.contains(&n.person_id) && ids.insert(n.id.clone()),
            TaskErrorCode::Forbidden,
        )?;
        for (s, max) in [
            (&n.title, 256),
            (&n.instruction, 8000),
            (&n.deliverable, 4000),
            (&n.acceptance, 4000),
        ] {
            bounded(s, max)?;
        }
        check(
            n.dependencies.len() <= 128
                && n.dependencies.iter().collect::<BTreeSet<_>>().len() == n.dependencies.len(),
            TaskErrorCode::Invalid,
        )?;
    }
    for n in nodes {
        check(
            n.dependencies.iter().all(|d| ids.contains(d) && d != &n.id),
            TaskErrorCode::Invalid,
        )?;
    }
    let mut visited = BTreeSet::new();
    loop {
        let before = visited.len();
        for n in nodes {
            if n.dependencies.iter().all(|d| visited.contains(d)) {
                visited.insert(n.id.clone());
            }
        }
        if visited.len() == nodes.len() {
            return Ok(());
        }
        check(visited.len() > before, TaskErrorCode::Invalid)?;
    }
}
#[cfg(test)]
#[path = "task_machine_tests.rs"]
mod tests;
