//! Durable scenario-neutral coordination through the single collaboration writer.
use crate::*;
use contracts::{
    ActivationState, CandidateState, EvaluationState, GoalEventKind, IterationState, RoundState,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(crate) enum IterationCommand {
    Change(Box<EvolutionChange>),
    Get {
        run_id: String,
    },
    List {
        group: String,
        scenario: String,
        after: String,
        active: bool,
    },
}

impl Hub {
    pub async fn evolution_run(&self, run_id: &str) -> Result<EvolutionRun> {
        Ok(serde_json::from_value(
            self.control(Control::Evolution(IterationCommand::Get {
                run_id: run_id.into(),
            }))
            .await?,
        )?)
    }
    pub async fn evolution_change(&self, change: EvolutionChange) -> Result<EvolutionRun> {
        Ok(serde_json::from_value(
            self.control(Control::Evolution(IterationCommand::Change(Box::new(
                change,
            ))))
            .await?,
        )?)
    }
    pub async fn evolution_runs(
        &self,
        group: &str,
        scenario: &str,
        after: &str,
        active: bool,
    ) -> Result<Value> {
        self.control(Control::Evolution(IterationCommand::List {
            group: group.into(),
            scenario: scenario.into(),
            after: after.into(),
            active,
        }))
        .await
    }
}

fn read(db: &Connection, run_id: &str) -> Result<EvolutionRun> {
    let value: String = db.query_row(
        "SELECT value FROM evolution_runs WHERE id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str(&value)?)
}

fn version(
    db: &Connection,
    node_id: &str,
    subject_id: &str,
    group_id: &str,
) -> Result<EvolutionNode> {
    id(node_id)?;
    let (value, group): (String, String) = db.query_row(
        "SELECT n.value,t.group_id FROM research_nodes n JOIN research_subjects s ON s.id=n.subject_id JOIN research_topics t ON t.id=s.topic_id WHERE n.id=?1 AND n.subject_id=?2",
        params![node_id,subject_id], |row| Ok((row.get(0)?,row.get(1)?)))?;
    ensure!(group == group_id, "version belongs to another conversation");
    Ok(serde_json::from_str(&value)?)
}

fn running(state: IterationState) -> bool {
    matches!(state, IterationState::Running | IterationState::Paused)
}

/// Every claim and result is fenced by its durable iteration, not just its goal.
pub(crate) fn work_deadline(db: &Connection, goal_id: &str, claim: bool) -> Result<Option<u64>> {
    let binding: Option<(String, usize)> = db
        .query_row(
            "SELECT run_id,round FROM evolution_goal_bindings WHERE goal_id=?1",
            [goal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((run_id, number)) = binding else {
        return Ok(None);
    };
    let run = read(db, &run_id)?;
    ensure!(
        running(run.state) && (!claim || run.state == IterationState::Running),
        "iteration no longer permits this execution"
    );
    let round = run.rounds.last().context("iteration round missing")?;
    ensure!(
        round.number as usize == number && round.goal_id == goal_id && round.finished_ms.is_none(),
        "iteration round superseded"
    );
    let deadline = round
        .started_ms
        .saturating_add(run.input.max_seconds * 1000);
    ensure!(
        now_ms() < deadline,
        "iteration round execution limit expired"
    );
    Ok(Some(deadline))
}

fn round_mut(run: &mut EvolutionRun) -> Result<&mut EvolutionRound> {
    ensure!(running(run.state), "iteration is terminal");
    let round = run
        .rounds
        .last_mut()
        .context("iteration has no current round")?;
    ensure!(round.finished_ms.is_none(), "iteration round is terminal");
    Ok(round)
}

fn current_goal(db: &Connection, round: &EvolutionRound) -> Result<Goal> {
    let goal = goals::read(db, &round.goal_id)?;
    ensure!(
        goal.work.len() == round.goal_attempts.len()
            && goal
                .work
                .iter()
                .all(|work| round.goal_attempts.get(&work.id) == Some(&work.attempt)),
        "iteration goal attempt changed"
    );
    Ok(goal)
}

/// Generic candidate assessment uses durable execution and host acceptance.
/// Scenario-specific evidence is checked before these records are admitted.
pub(crate) fn assessment(
    db: &Connection,
    node: &EvolutionNode,
    goal: &Goal,
) -> Result<Option<contracts::EvolutionState>> {
    let binding: Option<(String, usize)> = db
        .query_row(
            "SELECT run_id,round FROM evolution_goal_bindings WHERE goal_id=?1",
            [&goal.input.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((run_id, number)) = binding else {
        return Ok(None);
    };
    let run = read(db, &run_id)?;
    let Some(round) = number
        .checked_sub(1)
        .and_then(|index| run.rounds.get(index))
    else {
        return Ok(Some(contracts::EvolutionState::Unassessed));
    };
    let Some(candidate) = round
        .candidate
        .as_ref()
        .filter(|candidate| candidate.version_id == node.input.id)
    else {
        return Ok(None);
    };
    if node.goal_attempts.as_ref() != Some(&round.goal_attempts)
        || round.goal_id != goal.input.id
        || goal.work.len() != round.goal_attempts.len()
        || !goal
            .work
            .iter()
            .all(|work| round.goal_attempts.get(&work.id) == Some(&work.attempt))
    {
        return Ok(Some(contracts::EvolutionState::Unassessed));
    }
    Ok(Some(match candidate.state {
        CandidateState::Accepted
            if goal.state == GoalState::Completed
                && round
                    .evaluation
                    .as_ref()
                    .is_some_and(|evaluation| evaluation.state == EvaluationState::Passed) =>
        {
            contracts::EvolutionState::Accepted
        }
        CandidateState::Rejected => contracts::EvolutionState::Failed,
        CandidateState::Proposed
            if goal.state == GoalState::Active && round.evaluation.is_some() =>
        {
            contracts::EvolutionState::Submitted
        }
        _ if goal.state == GoalState::Cancelled => contracts::EvolutionState::Cancelled,
        _ => contracts::EvolutionState::Unassessed,
    }))
}

fn evidence(value: &Value, max_bytes: usize) -> Result<()> {
    ensure!(
        value.is_object()
            && value.as_object().is_some_and(|v| !v.is_empty())
            && serde_json::to_vec(value)?.len() <= max_bytes,
        "structured bounded evidence required"
    );
    Ok(())
}

fn submitted<'a>(goal: &'a Goal, work_id: &str) -> Result<&'a WorkItem> {
    let work = goal
        .work
        .iter()
        .find(|work| work.id == work_id)
        .context("unknown evidence assignment")?;
    ensure!(
        work.state == WorkState::Submitted
            && work.result.as_ref().is_some_and(|result| result.succeeded),
        "evidence assignment must have a successful submission"
    );
    Ok(work)
}

fn create(
    db: &Transaction<'_>,
    input: EvolutionRunInput,
    change: &EvolutionChange,
) -> Result<EvolutionRun> {
    ensure!(
        input.id == change.run_id && change.revision == 0,
        "iteration creation identity mismatch"
    );
    id(&input.subject_id)?;
    ensure!(
        (1..=256).contains(&input.max_rounds),
        "iteration round bound must be 1..256"
    );
    ensure!(
        input.total_budget_seconds >= input.max_seconds
            && input.total_budget_seconds
                <= input
                    .max_seconds
                    .saturating_mul(u64::from(input.max_rounds)),
        "iteration budget must cover a round and remain within its round bound"
    );
    text(&input.objective, 12000)?;
    text(&input.title, 220)?;
    goals::available(db, &input.group_id)?;
    for assignment in &input.assignments {
        goals::member(db, &input.group_id, &assignment.person_id)?;
    }
    version(
        db,
        &input.baseline_version_id,
        &input.subject_id,
        &input.group_id,
    )?;
    ensure!(
        !db.query_row(
            "SELECT EXISTS(SELECT 1 FROM evolution_runs WHERE id=?1)",
            [&input.id],
            |row| row.get::<_, bool>(0)
        )?,
        "iteration ID already exists"
    );
    ensure!(
        db.query_row("SELECT COUNT(*) FROM evolution_runs", [], |row| row
            .get::<_, u64>(0))?
            < 2048,
        "iteration capacity exhausted"
    );
    let run = EvolutionRun {
        baseline_version_id: input.baseline_version_id.clone(),
        input,
        state: IterationState::Running,
        revision: 1,
        created_ms: now_ms(),
        updated_ms: now_ms(),
        used_seconds: 0,
        rounds: vec![],
    };
    run.next_goal()?;
    db.execute(
        "INSERT INTO evolution_runs(id,group_id,scenario,active,value) VALUES(?1,?2,?3,1,?4)",
        params![
            run.input.id,
            run.input.group_id,
            run.input.scenario_id,
            serde_json::to_string(&run)?
        ],
    )?;
    Ok(run)
}

fn start_round(db: &Transaction<'_>, run: &mut EvolutionRun, input: GoalInput) -> Result<()> {
    ensure!(
        run.state == IterationState::Running,
        "iteration is not running"
    );
    ensure!(
        run.rounds
            .last()
            .is_none_or(|round| round.finished_ms.is_some()),
        "iteration already has an active round"
    );
    ensure!(
        run.rounds.len() < run.input.max_rounds as usize,
        "iteration round budget exhausted"
    );
    ensure!(
        run.input
            .total_budget_seconds
            .saturating_sub(run.used_seconds)
            >= run.input.max_seconds,
        "iteration execution budget exhausted"
    );
    ensure!(
        input == run.next_goal()?,
        "iteration goal must match frozen input and current feedback"
    );
    let goal = goals::create_goal(db, input)?;
    ensure!(
        goal.state == GoalState::Active
            && goal
                .work
                .iter()
                .all(|work| work.attempt == 1 && work.state == WorkState::Ready),
        "iteration goal already executed outside its round"
    );
    run.rounds.push(EvolutionRound {
        number: run.rounds.len() as u32 + 1,
        state: RoundState::Running,
        goal_id: goal.input.id,
        goal_attempts: goal
            .work
            .iter()
            .map(|work| (work.id.clone(), work.attempt))
            .collect(),
        baseline_version_id: run.baseline_version_id.clone(),
        started_ms: now_ms(),
        finished_ms: None,
        feedback: String::new(),
        candidate: None,
        evaluation: None,
        activation: None,
    });
    let round = run.rounds.last().unwrap();
    db.execute(
        "INSERT INTO evolution_goal_bindings(goal_id,run_id,round) VALUES(?1,?2,?3)",
        params![round.goal_id, run.input.id, round.number],
    )?;
    Ok(())
}

fn candidate(
    db: &Transaction<'_>,
    run: &mut EvolutionRun,
    version_id: String,
    producer_work_id: String,
) -> Result<()> {
    let node = version(db, &version_id, &run.input.subject_id, &run.input.group_id)?;
    let round = round_mut(run)?;
    ensure!(
        round.state == RoundState::Running && round.candidate.is_none(),
        "round candidate is immutable"
    );
    let goal = current_goal(db, round)?;
    submitted(&goal, &producer_work_id)?;
    ensure!(
        node.input.goal_id.as_deref() == Some(round.goal_id.as_str())
            && node.goal_attempts.as_ref() == Some(&round.goal_attempts),
        "candidate does not bind the current goal attempt"
    );
    ensure!(
        node.input.parents.contains(&round.baseline_version_id),
        "candidate does not descend from the current baseline"
    );
    round.candidate = Some(EvolutionCandidate {
        version_id,
        producer_work_id,
        state: CandidateState::Proposed,
    });
    Ok(())
}

fn evaluate(
    db: &Transaction<'_>,
    run: &mut EvolutionRun,
    evaluation: CandidateEvaluation,
) -> Result<()> {
    text(&evaluation.summary, 2000)?;
    evidence(&evaluation.evidence, 8192)?;
    let round = round_mut(run)?;
    ensure!(
        round.state == RoundState::Running && round.evaluation.is_none(),
        "round evaluation is immutable"
    );
    let candidate = round
        .candidate
        .as_ref()
        .context("evaluation requires a candidate")?;
    let goal = current_goal(db, round)?;
    ensure!(
        goal.state == GoalState::Active && goal.revision == evaluation.goal_revision,
        "evaluation goal revision conflict"
    );
    let producer = submitted(&goal, &candidate.producer_work_id)?;
    let evaluator = submitted(&goal, &evaluation.evaluator_work_id)?;
    ensure!(
        producer.person_id != evaluator.person_id,
        "candidate producer cannot independently evaluate its own candidate"
    );
    ensure!(
        evaluator.result.as_ref().is_some_and(|result| {
            result.evidence == evaluation.evidence
                || (evaluation.evidence.get("work_evidence") == Some(&result.evidence)
                    && evaluation
                        .evidence
                        .get("host_evidence")
                        .is_some_and(|v| v.as_object().is_some_and(|object| !object.is_empty())))
        }),
        "evaluation must bind recorded evaluator evidence"
    );
    if evaluation.state == EvaluationState::Passed {
        ensure!(
            goal.work
                .iter()
                .all(|work| work.state == WorkState::Submitted
                    && work.result.as_ref().is_some_and(|result| result.succeeded)),
            "passing evaluation requires every current assignment submission"
        );
    }
    round.state = RoundState::Assessed;
    if evaluation.state == EvaluationState::Failed {
        round.candidate.as_mut().unwrap().state = CandidateState::Rejected;
    }
    round.evaluation = Some(evaluation);
    Ok(())
}

fn accept(db: &Transaction<'_>, run: &mut EvolutionRun) -> Result<()> {
    let round = round_mut(run)?;
    ensure!(
        round.state == RoundState::Assessed,
        "acceptance requires an assessed round"
    );
    let evaluation = round
        .evaluation
        .as_ref()
        .context("acceptance requires independent evaluation")?;
    let mut goal = current_goal(db, round)?;
    ensure!(
        evaluation.state == EvaluationState::Passed
            && evaluation.goal_revision == goal.revision
            && goal.state == GoalState::Active,
        "acceptance requires unchanged passing evidence"
    );
    ensure!(
        goal.work
            .iter()
            .all(|work| work.state == WorkState::Submitted),
        "acceptance requires all assignments submitted"
    );
    goal.state = GoalState::Completed;
    goal.revision += 1;
    goals::save(db, &goal, GoalEventKind::Accepted, None)?;
    round.candidate.as_mut().context("candidate missing")?.state = CandidateState::Accepted;
    round.state = RoundState::Accepted;
    Ok(())
}

fn pending_activation(run: &EvolutionRun) -> bool {
    run.rounds
        .last()
        .and_then(|round| round.activation.as_ref())
        .is_some_and(|activation| activation.state == ActivationState::Pending)
}

fn begin_activation(
    db: &Transaction<'_>,
    run: &mut EvolutionRun,
    claim_id: String,
    max_seconds: u64,
) -> Result<()> {
    id(&claim_id)?;
    ensure!(
        (1..=3600).contains(&max_seconds),
        "activation bound must be 1..3600 seconds"
    );
    ensure!(
        run.state == IterationState::Running,
        "activation requires a running iteration"
    );
    let round = round_mut(run)?;
    ensure!(
        round.state == RoundState::Accepted && round.activation.is_none(),
        "activation admission requires an accepted round without prior activation"
    );
    ensure!(
        round
            .candidate
            .as_ref()
            .is_some_and(|candidate| candidate.state == CandidateState::Accepted)
            && current_goal(db, round)?.state == GoalState::Completed,
        "activation requires current accepted candidate and goal evidence"
    );
    round.activation = Some(CandidateActivation {
        state: ActivationState::Pending,
        reference: claim_id.clone(),
        evidence: json!({"activation_claim_id":claim_id,"deadline_ms":now_ms().saturating_add(max_seconds * 1000)}),
    });
    Ok(())
}

fn activate(
    db: &Transaction<'_>,
    run: &mut EvolutionRun,
    activation: CandidateActivation,
) -> Result<()> {
    text(&activation.reference, 2048)?;
    ensure!(
        !activation.reference.trim().is_empty(),
        "activation reference required"
    );
    ensure!(
        activation.state != ActivationState::Pending,
        "pending activation requires atomic admission"
    );
    evidence(&activation.evidence, 4096)?;
    let round = round_mut(run)?;
    ensure!(
        round.state == RoundState::Accepted,
        "activation requires host acceptance"
    );
    ensure!(
        current_goal(db, round)?.state == GoalState::Completed,
        "activation requires current accepted goal evidence"
    );
    match round.activation.as_ref() {
        Some(pending) => ensure!(
            pending.state == ActivationState::Pending
                && activation.evidence["activation_claim_id"].as_str()
                    == Some(pending.reference.as_str()),
            "activation settlement requires its pending claim and is immutable"
        ),
        None => ensure!(
            activation.state == ActivationState::Failed
                && activation.evidence.get("activation_claim_id").is_none(),
            "successful activation or rollback requires atomic admission"
        ),
    }
    round.activation = Some(activation);
    Ok(())
}

fn finish(
    db: &Transaction<'_>,
    run: &mut EvolutionRun,
    state: RoundState,
    feedback: String,
) -> Result<()> {
    text(&feedback, 2000)?;
    ensure!(!feedback.trim().is_empty(), "iteration feedback required");
    ensure!(
        matches!(
            state,
            RoundState::Completed | RoundState::Failed | RoundState::NoImprovement
        ),
        "finish requires a terminal round state"
    );
    ensure!(
        !pending_activation(run),
        "activation must settle before finishing its round"
    );
    let round = round_mut(run)?;
    let mut goal = current_goal(db, round)?;
    ensure!(
        goal.work.iter().all(|work| work.state != WorkState::Running
            || work
                .deadline_ms
                .is_some_and(|deadline| deadline <= now_ms())),
        "running assignments must finish or expire before completing a round"
    );
    match state {
        RoundState::Completed => {
            ensure!(
                round.state == RoundState::Accepted
                    && round
                        .candidate
                        .as_ref()
                        .is_some_and(|candidate| candidate.state == CandidateState::Accepted),
                "completed round requires host acceptance"
            );
            ensure!(
                round
                    .activation
                    .as_ref()
                    .is_none_or(|activation| activation.state == ActivationState::Active),
                "failed activation cannot advance the baseline"
            );
        }
        RoundState::NoImprovement => ensure!(
            round.candidate.is_none()
                && goal
                    .work
                    .iter()
                    .all(|work| work.state == WorkState::Submitted),
            "no-improvement round requires completed exploration without a candidate"
        ),
        RoundState::Failed => ensure!(
            round.state != RoundState::Accepted
                || round
                    .activation
                    .as_ref()
                    .is_some_and(|activation| activation.state != ActivationState::Active),
            "accepted round cannot fail without recorded activation failure"
        ),
        _ => unreachable!(),
    }
    if state != RoundState::Completed && goal.state == GoalState::Active {
        goal.state = GoalState::Cancelled;
        goal.revision += 1;
        goals::save(db, &goal, GoalEventKind::Cancelled, None)?;
    }
    round.state = state;
    round.feedback = feedback;
    round.finished_ms = Some(now_ms());
    let elapsed = now_ms()
        .saturating_sub(round.started_ms)
        .div_ceil(1000)
        .max(1);
    let accepted_version = (state == RoundState::Completed)
        .then(|| round.candidate.as_ref().unwrap().version_id.clone());
    run.used_seconds = run
        .used_seconds
        .saturating_add(elapsed.min(run.input.max_seconds));
    if let Some(version) = accepted_version {
        run.baseline_version_id = version;
    }
    if run.rounds.len() >= run.input.max_rounds as usize {
        run.state = IterationState::Completed;
    } else if run
        .input
        .total_budget_seconds
        .saturating_sub(run.used_seconds)
        < run.input.max_seconds
    {
        run.state = IterationState::BudgetExhausted;
    }
    Ok(())
}

impl Store {
    pub(crate) fn iteration_command(&mut self, command: IterationCommand) -> Result<Value> {
        match command {
            IterationCommand::Change(change) => self.iteration_change(*change),
            IterationCommand::Get { run_id } => {
                id(&run_id)?;
                Ok(json!(read(&self.connection, &run_id)?))
            }
            IterationCommand::List {
                group,
                scenario,
                after,
                active,
            } => {
                ensure!(
                    group.len() <= 100 && scenario.len() <= 100 && after.len() <= 100,
                    "invalid iteration cursor"
                );
                let mut statement = self.connection.prepare("SELECT value FROM evolution_runs WHERE (?1='' OR group_id=?1) AND (?2='' OR scenario=?2) AND id>?3 AND (?4=0 OR active=1) ORDER BY id LIMIT 21")?;
                let mut runs = vec![];
                let mut bytes = 0;
                let mut more = false;
                for row in statement.query_map(params![group, scenario, after, active], |row| {
                    row.get::<_, String>(0)
                })? {
                    let value = row?;
                    if runs.len() == 20
                        || (!runs.is_empty() && bytes + value.len() > 4 * 1024 * 1024)
                    {
                        more = true;
                        break;
                    }
                    bytes += value.len();
                    runs.push(serde_json::from_str::<EvolutionRun>(&value)?);
                }
                let next = more.then(|| runs.last().unwrap().input.id.clone());
                Ok(json!({"runs":runs,"next_after":next}))
            }
        }
    }

    fn iteration_change(&mut self, change: EvolutionChange) -> Result<Value> {
        id(&change.run_id)?;
        id(&change.request_id)?;
        let digest = storage::digest(&serde_json::to_vec(&change)?);
        let tx = self.connection.transaction()?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT digest FROM evolution_requests WHERE run_id=?1 AND request_id=?2",
                params![change.run_id, change.request_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(previous) = previous {
            ensure!(
                previous == digest,
                "iteration request ID reused with different input"
            );
            return Ok(json!(read(&tx, &change.run_id)?));
        }
        let mut run = if let EvolutionAction::Create { input } = &change.action {
            create(&tx, input.clone(), &change)?
        } else {
            let mut run = read(&tx, &change.run_id)?;
            ensure!(
                run.revision == change.revision,
                "iteration revision conflict"
            );
            goals::available(&tx, &run.input.group_id)?;
            match change.action {
                EvolutionAction::Pause => {
                    ensure!(
                        !pending_activation(&run),
                        "activation must settle before pausing"
                    );
                    ensure!(
                        run.state == IterationState::Running,
                        "only running iterations can pause"
                    );
                    run.state = IterationState::Paused;
                }
                EvolutionAction::Resume => {
                    ensure!(
                        run.state == IterationState::Paused,
                        "only paused iterations can resume"
                    );
                    run.state = IterationState::Running;
                }
                EvolutionAction::Stop => {
                    ensure!(
                        !pending_activation(&run),
                        "activation must settle before stopping"
                    );
                    ensure!(running(run.state), "iteration is terminal");
                    if let Some(round) = run
                        .rounds
                        .last_mut()
                        .filter(|round| round.finished_ms.is_none())
                    {
                        let mut goal = goals::read(&tx, &round.goal_id)?;
                        if goal.state == GoalState::Active {
                            goal.state = GoalState::Cancelled;
                            goal.revision += 1;
                            goals::save(&tx, &goal, GoalEventKind::Cancelled, None)?;
                        }
                        round.state = RoundState::Cancelled;
                        round.finished_ms = Some(now_ms());
                        round.feedback = "Iteration stopped by host.".into();
                        run.used_seconds = run.used_seconds.saturating_add(
                            now_ms()
                                .saturating_sub(round.started_ms)
                                .div_ceil(1000)
                                .max(1)
                                .min(run.input.max_seconds),
                        );
                    }
                    run.state = IterationState::Stopped;
                }
                EvolutionAction::StartRound { goal } => start_round(&tx, &mut run, goal)?,
                EvolutionAction::RecordCandidate {
                    version_id,
                    producer_work_id,
                } => candidate(&tx, &mut run, version_id, producer_work_id)?,
                EvolutionAction::Evaluate { evaluation } => evaluate(&tx, &mut run, evaluation)?,
                EvolutionAction::Accept => accept(&tx, &mut run)?,
                EvolutionAction::BeginActivation {
                    claim_id,
                    max_seconds,
                } => begin_activation(&tx, &mut run, claim_id, max_seconds)?,
                EvolutionAction::Activate { activation } => activate(&tx, &mut run, activation)?,
                EvolutionAction::FinishRound { state, feedback } => {
                    finish(&tx, &mut run, state, feedback)?
                }
                EvolutionAction::Create { .. } => unreachable!(),
            }
            run.revision = run
                .revision
                .checked_add(1)
                .context("iteration revision exhausted")?;
            run
        };
        run.updated_ms = now_ms();
        let encoded = serde_json::to_string(&run)?;
        ensure!(
            encoded.len() <= 6 * 1024 * 1024,
            "iteration history exceeds bound"
        );
        tx.execute(
            "UPDATE evolution_runs SET active=?2,value=?3 WHERE id=?1",
            params![run.input.id, running(run.state), encoded],
        )?;
        tx.execute(
            "INSERT INTO evolution_requests(run_id,request_id,digest) VALUES(?1,?2,?3)",
            params![change.run_id, change.request_id, digest],
        )?;
        tx.commit()?;
        Ok(json!(run))
    }
}
