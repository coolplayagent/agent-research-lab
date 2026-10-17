use super::*;
use contracts::{
    CandidateState, EvaluationState, IterationState, RoundState, SubjectKind, TopicState,
};

async fn setup() -> (tempfile::TempDir, Hub, EvolutionRunInput) {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["maker".into(), "reviewer".into()])
        .await
        .unwrap();
    hub.create_group(NewGroup {
        id: "group".into(),
        title: "Planning".into(),
        topic: "Improve a structured proposal".into(),
        kind: GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), "maker".into(), "reviewer".into()],
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Topic {
        topic: CollaborationTopic {
            id: "topic".into(),
            group_id: "group".into(),
            title: "Plans".into(),
            objective: "Compare plans".into(),
            state: TopicState::Active,
            revision: 0,
        },
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Subject {
        subject: CollaborationSubject {
            id: "subject".into(),
            topic_id: "topic".into(),
            name: "A plan".into(),
            kind: SubjectKind::Other,
            reference: "plan/example".into(),
            revision: 0,
        },
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Node {
        node: EvolutionInput {
            id: "baseline".into(),
            subject_id: "subject".into(),
            parents: vec![],
            version: "1".into(),
            reference: "plan/example/1".into(),
            goal_id: None,
            pins: EvidencePins::default(),
        },
    })
    .await
    .unwrap();
    let input = EvolutionRunInput {
        id: "iteration".into(),
        group_id: "group".into(),
        subject_id: "subject".into(),
        scenario_id: "plan-comparison".into(),
        title: "Improve plan".into(),
        objective: "Reduce the plan cost".into(),
        acceptance: "Independent evaluation reports a lower cost".into(),
        assignments: vec![
            AssignmentInput {
                person_id: "maker".into(),
                instruction: "Create a candidate plan".into(),
            },
            AssignmentInput {
                person_id: "reviewer".into(),
                instruction: "Independently evaluate cost".into(),
            },
        ],
        baseline_version_id: "baseline".into(),
        parameters: json!({"max_cost":10}),
        max_rounds: 2,
        max_seconds: 60,
        total_budget_seconds: 120,
    };
    (root, hub, input)
}

async fn change(hub: &Hub, run: &EvolutionRun, action: EvolutionAction) -> EvolutionRun {
    hub.evolution_change(EvolutionChange {
        request_id: format!("change-{}", run.revision),
        run_id: run.input.id.clone(),
        revision: run.revision,
        action,
    })
    .await
    .unwrap()
}

async fn create_run(hub: &Hub, input: EvolutionRunInput) -> EvolutionRun {
    hub.evolution_change(EvolutionChange {
        request_id: "create".into(),
        run_id: input.id.clone(),
        revision: 0,
        action: EvolutionAction::Create { input },
    })
    .await
    .unwrap()
}

async fn start(hub: &Hub, run: &EvolutionRun) -> EvolutionRun {
    change(
        hub,
        run,
        EvolutionAction::StartRound {
            goal: run.next_goal().unwrap(),
        },
    )
    .await
}

async fn claim(hub: &Hub, run: &EvolutionRun, index: usize) -> Result<Goal> {
    hub.work_as(
        &run.input.assignments[index].person_id,
        WorkChange::Claim {
            attempt: 1,
            goal_id: run.rounds.last().unwrap().goal_id.clone(),
            work_id: format!("work-{index}"),
            claim_id: format!("claim-{index}"),
        },
    )
    .await
}

async fn submit(hub: &Hub, run: &EvolutionRun, index: usize) -> Goal {
    hub.work_as(
        &run.input.assignments[index].person_id,
        WorkChange::Submit {
            attempt: 1,
            goal_id: run.rounds.last().unwrap().goal_id.clone(),
            work_id: format!("work-{index}"),
            claim_id: format!("claim-{index}"),
            result: WorkResult {
                code: None,
                succeeded: true,
                summary: "A lower cost was reproduced".into(),
                evidence: json!({"candidate_cost":8,"baseline_cost":10}),
            },
        },
    )
    .await
    .unwrap()
}

async fn candidate(hub: &Hub, run: &EvolutionRun) -> (EvolutionRun, Goal) {
    claim(hub, run, 0).await.unwrap();
    submit(hub, run, 0).await;
    claim(hub, run, 1).await.unwrap();
    let goal = submit(hub, run, 1).await;
    hub.research_change(ResearchChange::Node {
        node: EvolutionInput {
            id: "candidate".into(),
            subject_id: "subject".into(),
            parents: vec!["baseline".into()],
            version: "2".into(),
            reference: "plan/example/2".into(),
            goal_id: Some(goal.input.id.clone()),
            pins: EvidencePins::default(),
        },
    })
    .await
    .unwrap();
    (
        change(
            hub,
            run,
            EvolutionAction::RecordCandidate {
                version_id: "candidate".into(),
                producer_work_id: "work-0".into(),
            },
        )
        .await,
        goal,
    )
}

#[tokio::test]
async fn iteration_acceptance_is_independent_durable_and_replay_safe() {
    let (root, hub, input) = setup().await;
    let created = create_run(&hub, input).await;
    let start_request = EvolutionChange {
        request_id: "start".into(),
        run_id: created.input.id.clone(),
        revision: created.revision,
        action: EvolutionAction::StartRound {
            goal: created.next_goal().unwrap(),
        },
    };
    let started = hub.evolution_change(start_request.clone()).await.unwrap();
    let replay = hub.evolution_change(start_request.clone()).await.unwrap();
    assert_eq!(replay.revision, started.revision);
    assert_eq!(
        hub.goals("group", "", "", false).await.unwrap()["goals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (recorded, goal) = candidate(&hub, &started).await;
    let evaluation = CandidateEvaluation {
        evaluator_work_id: "work-1".into(),
        goal_revision: goal.revision,
        state: EvaluationState::Passed,
        summary: "Independent cost calculation passed".into(),
        evidence: goal.work[1].result.as_ref().unwrap().evidence.clone(),
    };
    let assessed = change(&hub, &recorded, EvolutionAction::Evaluate { evaluation }).await;
    assert_eq!(assessed.baseline_version_id, "baseline");
    assert_eq!(
        assessed.rounds[0].candidate.as_ref().unwrap().state,
        CandidateState::Proposed
    );
    let accepted = change(&hub, &assessed, EvolutionAction::Accept).await;
    let completed = change(
        &hub,
        &accepted,
        EvolutionAction::FinishRound {
            state: RoundState::Completed,
            feedback: "Retain the cheaper plan and reduce its scheduling risk".into(),
        },
    )
    .await;
    assert_eq!(completed.baseline_version_id, "candidate");
    assert_eq!(completed.state, IterationState::Running);
    assert!(completed.used_seconds >= 1);
    drop(hub);
    let hub = Hub::open(root.path()).unwrap();
    let restored = hub.evolution_run("iteration").await.unwrap();
    assert_eq!(restored.revision, completed.revision);
    assert!(
        restored
            .next_goal()
            .unwrap()
            .objective
            .contains("scheduling risk")
    );
    assert!(
        restored
            .next_goal()
            .unwrap()
            .objective
            .contains("candidate")
    );
    let replay = hub.evolution_change(start_request).await.unwrap();
    assert_eq!(replay.revision, completed.revision);
    let second = start(&hub, &restored).await;
    assert_eq!(second.rounds.len(), 2);
    assert_eq!(second.rounds[1].baseline_version_id, "candidate");
}

#[tokio::test]
async fn self_review_forged_evidence_stale_revision_and_premature_acceptance_fail() {
    let (_root, hub, input) = setup().await;
    let run = start(&hub, &create_run(&hub, input).await).await;
    let (run, goal) = candidate(&hub, &run).await;
    assert!(
        hub.change_goal(GoalChange::Accept {
            goal_id: goal.input.id.clone(),
            revision: goal.revision
        })
        .await
        .is_err()
    );
    assert!(
        hub.change_goal(GoalChange::Cancel {
            goal_id: goal.input.id.clone(),
            revision: goal.revision
        })
        .await
        .is_err()
    );
    assert_eq!(
        hub.goals("group", "", "", false).await.unwrap()["goals"][0]["iteration_id"],
        "iteration"
    );
    let base = CandidateEvaluation {
        evaluator_work_id: "work-1".into(),
        goal_revision: goal.revision,
        state: EvaluationState::Passed,
        summary: "Verified".into(),
        evidence: goal.work[1].result.as_ref().unwrap().evidence.clone(),
    };
    for (index, evaluation) in [
        CandidateEvaluation {
            evaluator_work_id: "work-0".into(),
            ..base.clone()
        },
        CandidateEvaluation {
            evidence: json!({"invented":true}),
            ..base.clone()
        },
        CandidateEvaluation {
            goal_revision: goal.revision - 1,
            ..base
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            hub.evolution_change(EvolutionChange {
                request_id: format!("invalid-{index}"),
                run_id: run.input.id.clone(),
                revision: run.revision,
                action: EvolutionAction::Evaluate { evaluation }
            })
            .await
            .is_err()
        );
    }
    for (index, action) in [
        EvolutionAction::Accept,
        EvolutionAction::FinishRound {
            state: RoundState::Completed,
            feedback: "Claimed acceptance".into(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            hub.evolution_change(EvolutionChange {
                request_id: format!("premature-{index}"),
                run_id: run.input.id.clone(),
                revision: run.revision,
                action
            })
            .await
            .is_err()
        );
    }
    assert_eq!(
        hub.evolution_run("iteration").await.unwrap().revision,
        run.revision
    );
}

#[tokio::test]
async fn pause_and_stop_fence_admission_claims_and_late_results() {
    let (_root, hub, input) = setup().await;
    let run = start(&hub, &create_run(&hub, input).await).await;
    claim(&hub, &run, 0).await.unwrap();
    let paused = change(&hub, &run, EvolutionAction::Pause).await;
    assert!(claim(&hub, &paused, 1).await.is_err());
    submit(&hub, &paused, 0).await;
    let resumed = change(&hub, &paused, EvolutionAction::Resume).await;
    claim(&hub, &resumed, 1).await.unwrap();
    let stopped = change(&hub, &resumed, EvolutionAction::Stop).await;
    assert!(claim(&hub, &stopped, 1).await.is_err());
    assert!(
        hub.work_as(
            "reviewer",
            WorkChange::Submit {
                attempt: 1,
                goal_id: stopped.rounds[0].goal_id.clone(),
                work_id: "work-1".into(),
                claim_id: "claim-1".into(),
                result: WorkResult {
                    code: None,
                    summary: "Late claim".into(),
                    succeeded: true,
                    evidence: json!({"score":1})
                }
            }
        )
        .await
        .is_err()
    );
    assert!(
        hub.evolution_runs("group", "", "", true).await.unwrap()["runs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn immutable_requests_policy_and_budget_prevent_repeated_execution() {
    let (_root, hub, mut input) = setup().await;
    input.total_budget_seconds = 60;
    let run = create_run(&hub, input.clone()).await;
    let mut changed_input = input;
    changed_input.acceptance = "Weaker acceptance".into();
    assert!(
        hub.evolution_change(EvolutionChange {
            request_id: "create".into(),
            run_id: "iteration".into(),
            revision: 0,
            action: EvolutionAction::Create {
                input: changed_input
            }
        })
        .await
        .is_err()
    );
    let mut changed_goal = run.next_goal().unwrap();
    changed_goal.parameters = json!({"max_cost":100});
    assert!(
        hub.evolution_change(EvolutionChange {
            request_id: "drift".into(),
            run_id: "iteration".into(),
            revision: run.revision,
            action: EvolutionAction::StartRound { goal: changed_goal }
        })
        .await
        .is_err()
    );
    let run = start(&hub, &run).await;
    for index in 0..2 {
        claim(&hub, &run, index).await.unwrap();
        submit(&hub, &run, index).await;
    }
    let run = change(
        &hub,
        &run,
        EvolutionAction::FinishRound {
            state: RoundState::NoImprovement,
            feedback: "No reproducible improvement found".into(),
        },
    )
    .await;
    assert_eq!(run.state, IterationState::BudgetExhausted);
    assert_eq!(run.baseline_version_id, "baseline");
    assert!(
        hub.evolution_change(EvolutionChange {
            request_id: "exhausted".into(),
            run_id: "iteration".into(),
            revision: run.revision,
            action: EvolutionAction::StartRound {
                goal: run.next_goal().unwrap()
            }
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn failed_round_cancels_ready_work_and_attempt_bindings_cannot_be_rewritten() {
    let (_root, hub, input) = setup().await;
    let run = start(&hub, &create_run(&hub, input).await).await;
    claim(&hub, &run, 0).await.unwrap();
    let goal = submit(&hub, &run, 0).await;
    assert!(
        hub.change_goal(GoalChange::Retry {
            goal_id: goal.input.id,
            revision: goal.revision,
            work_id: "work-0".into()
        })
        .await
        .is_err()
    );
    let run = change(
        &hub,
        &run,
        EvolutionAction::FinishRound {
            state: RoundState::Failed,
            feedback: "Required evaluator dependency is unavailable".into(),
        },
    )
    .await;
    assert!(claim(&hub, &run, 1).await.is_err());
    assert_eq!(
        hub.goals("group", "", "", true).await.unwrap()["goals"],
        json!([])
    );
    assert_eq!(run.baseline_version_id, "baseline");
    let next = start(&hub, &run).await;
    assert_ne!(next.rounds[0].goal_id, next.rounds[1].goal_id);
    assert_eq!(next.rounds[1].goal_attempts["work-0"], 1);
}

#[tokio::test]
async fn failed_activation_never_promotes_an_accepted_candidate() {
    let (_root, hub, input) = setup().await;
    let run = start(&hub, &create_run(&hub, input).await).await;
    let (run, goal) = candidate(&hub, &run).await;
    let run = change(&hub, &run, EvolutionAction::Evaluate { evaluation: CandidateEvaluation {
        evaluator_work_id: "work-1".into(), goal_revision: goal.revision, state: EvaluationState::Passed,
        summary: "Independent evidence and host checks passed".into(),
        evidence: json!({"work_evidence":goal.work[1].result.as_ref().unwrap().evidence,"host_evidence":{"check_digest":"fixed-host-evidence"}}),
    } }).await;
    let run = change(&hub, &run, EvolutionAction::Accept).await;
    let run = change(
        &hub,
        &run,
        EvolutionAction::BeginActivation {
            claim_id: "activation-proof".into(),
            max_seconds: 60,
        },
    )
    .await;
    let run = change(
        &hub,
        &run,
        EvolutionAction::Activate {
            activation: CandidateActivation {
                state: contracts::ActivationState::RolledBack,
                reference: "plan/example/1".into(),
                evidence: json!({"health_check":"failed","restored_baseline":"baseline","activation_claim_id":"activation-proof"}),
            },
        },
    )
    .await;
    assert!(
        hub.evolution_change(EvolutionChange {
            request_id: "false-promotion".into(),
            run_id: run.input.id.clone(),
            revision: run.revision,
            action: EvolutionAction::FinishRound {
                state: RoundState::Completed,
                feedback: "Incorrectly claim success".into()
            }
        })
        .await
        .is_err()
    );
    let run = change(
        &hub,
        &run,
        EvolutionAction::FinishRound {
            state: RoundState::Failed,
            feedback: "Candidate passed evaluation but activation failed and baseline was restored"
                .into(),
        },
    )
    .await;
    assert_eq!(run.baseline_version_id, "baseline");
    assert_eq!(
        run.rounds[0].candidate.as_ref().unwrap().state,
        CandidateState::Accepted
    );
    assert_eq!(run.rounds[0].state, RoundState::Failed);
}

#[tokio::test]
async fn activation_admission_fences_pause_stop_and_requires_its_terminal_receipt() {
    let (root, hub, input) = setup().await;
    let run = start(&hub, &create_run(&hub, input).await).await;
    let (run, goal) = candidate(&hub, &run).await;
    let run = change(
        &hub,
        &run,
        EvolutionAction::Evaluate {
            evaluation: CandidateEvaluation {
                evaluator_work_id: "work-1".into(),
                goal_revision: goal.revision,
                state: EvaluationState::Passed,
                summary: "Independent checks passed".into(),
                evidence: goal.work[1].result.as_ref().unwrap().evidence.clone(),
            },
        },
    )
    .await;
    let run = change(&hub, &run, EvolutionAction::Accept).await;
    let paused = change(&hub, &run, EvolutionAction::Pause).await;
    let admission = |revision| EvolutionChange {
        request_id: "activation-admission".into(),
        run_id: run.input.id.clone(),
        revision,
        action: EvolutionAction::BeginActivation {
            claim_id: "switch-claim".into(),
            max_seconds: 60,
        },
    };
    assert!(hub.evolution_change(admission(run.revision)).await.is_err());
    assert!(
        hub.evolution_change(admission(paused.revision))
            .await
            .is_err()
    );
    let run = change(&hub, &paused, EvolutionAction::Resume).await;
    let command = admission(run.revision);
    let run = hub.evolution_change(command.clone()).await.unwrap();
    assert_eq!(
        hub.evolution_change(command).await.unwrap().revision,
        run.revision
    );
    assert_eq!(
        run.rounds[0].activation.as_ref().unwrap().state,
        contracts::ActivationState::Pending
    );
    for action in [
        EvolutionAction::Pause,
        EvolutionAction::Stop,
        EvolutionAction::FinishRound {
            state: RoundState::Failed,
            feedback: "Unverified interruption".into(),
        },
        EvolutionAction::Activate {
            activation: CandidateActivation {
                state: contracts::ActivationState::Active,
                reference: "artifact".into(),
                evidence: json!({"activation_claim_id":"another-claim"}),
            },
        },
    ] {
        assert!(
            hub.evolution_change(EvolutionChange {
                request_id: "rejected".into(),
                run_id: run.input.id.clone(),
                revision: run.revision,
                action
            })
            .await
            .is_err()
        );
    }
    drop(hub);
    let hub = Hub::open(root.path()).unwrap();
    let run = hub.evolution_run(&run.input.id).await.unwrap();
    assert_eq!(
        run.rounds[0].activation.as_ref().unwrap().reference,
        "switch-claim"
    );
    let run = change(
        &hub,
        &run,
        EvolutionAction::Activate {
            activation: CandidateActivation {
                state: contracts::ActivationState::Active,
                reference: "artifact".into(),
                evidence: json!({"activation_claim_id":"switch-claim","health":"verified"}),
            },
        },
    )
    .await;
    let run = change(
        &hub,
        &run,
        EvolutionAction::FinishRound {
            state: RoundState::Completed,
            feedback: "Exact deployment verified".into(),
        },
    )
    .await;
    assert_eq!(run.baseline_version_id, "candidate");
}
