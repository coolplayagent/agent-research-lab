//! Deterministic non-code execution fixtures exercise the production coordinator
//! and versioned executor transport. No fixture adapter is registered in hosts.
use crate::*;
use collaboration_model::{
    CandidateEvaluation, CollaborationSubject, CollaborationTopic, EvidencePins, EvolutionAction,
    EvolutionChange, EvolutionInput, EvolutionRun, EvolutionRunInput, ResearchChange,
};
use contracts::{
    CandidateState, EvaluationState, GoalState, IterationState, RoundState, SubjectKind,
    TopicState, WorkState,
};
use service_api::{AgentOutput, ExecutionReceipt, ExecutorRequest};

struct Fixture {
    root: tempfile::TempDir,
    hub: collaboration_client::Client,
    registry: Arc<service_registry::Registry>,
    workers: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for worker in &self.workers {
            worker.abort();
        }
    }
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let hub: collaboration_client::Client = collaboration_storage_service::client(
            collaboration_store::Hub::open(&root.path().join("store")).unwrap(),
        );
        hub.register_operator().await.unwrap();
        hub.register(vec![
            "proposer-a".into(),
            "proposer-b".into(),
            "reviewer".into(),
        ])
        .await
        .unwrap();
        hub.create_group(collaboration_model::NewGroup {
            id: "designs".into(),
            title: "Capacity plans".into(),
            topic: "Compare structured plans".into(),
            kind: contracts::GroupKind::Conversation,
            private: false,
            members: vec![
                "operator".into(),
                "proposer-a".into(),
                "proposer-b".into(),
                "reviewer".into(),
            ],
        })
        .await
        .unwrap();
        hub.research_change(ResearchChange::Topic {
            topic: CollaborationTopic {
                id: "plans".into(),
                group_id: "designs".into(),
                title: "Plan quality".into(),
                objective: "Improve plan score".into(),
                state: TopicState::Active,
                revision: 0,
            },
        })
        .await
        .unwrap();
        hub.research_change(ResearchChange::Subject {
            subject: CollaborationSubject {
                id: "capacity".into(),
                topic_id: "plans".into(),
                name: "Capacity proposal".into(),
                kind: SubjectKind::Other,
                reference: "proposal/capacity".into(),
                revision: 0,
            },
        })
        .await
        .unwrap();
        hub.research_change(ResearchChange::Node {
            node: EvolutionInput {
                id: "baseline".into(),
                subject_id: "capacity".into(),
                parents: vec![],
                version: "v0".into(),
                reference: "proposal/v0".into(),
                goal_id: None,
                pins: EvidencePins::default(),
            },
        })
        .await
        .unwrap();
        let registry = Arc::new(service_registry::Registry::open(root.path()).unwrap());
        let mut config = registry.configuration();
        let executor = config
            .services
            .iter()
            .find(|service| service.kind == contracts::ServiceKind::Executor)
            .unwrap()
            .endpoint
            .clone();
        let sandbox = config
            .services
            .iter()
            .find(|service| service.kind == contracts::ServiceKind::Sandbox)
            .unwrap()
            .endpoint
            .clone();
        let workers = vec![
            Self::worker(executor, contracts::ServiceKind::Executor),
            Self::worker(sandbox, contracts::ServiceKind::Sandbox),
        ];
        let adapter = &mut config.adapters[0];
        adapter.enabled = true;
        adapter.agent = contracts::CodingAgent::Custom;
        adapter.command = vec!["/bin/true".into()];
        adapter.max_seconds = 30;
        for person in ["proposer-a", "proposer-b", "reviewer"] {
            config.bindings.insert(person.into(), adapter.id.clone());
        }
        registry.save(config).unwrap();
        Self {
            root,
            hub,
            registry,
            workers,
        }
    }

    fn worker(
        path: std::path::PathBuf,
        kind: contracts::ServiceKind,
    ) -> tokio::task::JoinHandle<()> {
        let listener = service_protocol::Listener::bind(&path).unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.listener.accept().await.unwrap();
                let envelope: service_protocol::Envelope<Value> =
                    service_protocol::read(&mut stream).await.unwrap();
                assert_eq!(envelope.protocol_version, service_protocol::VERSION);
                let result = if envelope.request["operation"] == "probe" {
                    json!(service_protocol::ServiceInfo {
                        protocol_version: service_protocol::VERSION,
                        service_id: format!("fixture-{kind}"),
                        kind,
                        capabilities: vec![
                            if kind == contracts::ServiceKind::Executor {
                                "json_stdio.v1"
                            } else {
                                "sandbox.run.v1"
                            }
                            .into()
                        ],
                        ready: true,
                    })
                } else {
                    let ExecutorRequest::Execute { adapter, input, .. } =
                        serde_json::from_value(envelope.request).unwrap()
                    else {
                        panic!("execution request required")
                    };
                    let round = if input.objective.contains("\"round\":2") {
                        2
                    } else {
                        1
                    };
                    if round == 2 {
                        assert!(input.objective.contains("score below threshold"));
                    }
                    let reviewer = input.person_id == "reviewer";
                    if reviewer {
                        assert!(input.instruction.contains("proposer-a"));
                        assert!(input.instruction.contains("proposer-b"));
                        assert!(input.instruction.contains("candidate_score"));
                    }
                    let evidence = if reviewer {
                        json!({"measured_score":round,"threshold":2})
                    } else {
                        json!({"candidate_score":round})
                    };
                    let output = AgentOutput {
                        protocol_version: service_protocol::VERSION,
                        request_id: input.request_id.clone(),
                        summary: format!("Plan score {round}"),
                        succeeded: true,
                        evidence,
                    };
                    json!(ExecutionReceipt {
                        input_sha256: storage::digest(&serde_json::to_vec(&input).unwrap()),
                        adapter_sha256: storage::digest(&serde_json::to_vec(&adapter).unwrap()),
                        output_sha256: storage::digest(&serde_json::to_vec(&output).unwrap()),
                        adapter_id: adapter.id,
                        agent: adapter.agent,
                        output,
                        sandbox_id: "deterministic-test-sandbox".into(),
                        elapsed_ms: 1,
                    })
                };
                service_protocol::write(
                    &mut stream,
                    &service_protocol::Reply::from_result(Ok(result)),
                )
                .await
                .unwrap();
            }
        })
    }

    async fn change(&self, run: &EvolutionRun, action: EvolutionAction) -> EvolutionRun {
        self.hub
            .evolution_change(EvolutionChange {
                request_id: format!("host-{}", run.revision),
                run_id: run.input.id.clone(),
                revision: run.revision,
                action,
            })
            .await
            .unwrap()
    }

    async fn create(&self, scenario_id: &str) -> EvolutionRun {
        self.create_named(scenario_id, "design-iteration").await
    }

    async fn create_named(&self, scenario_id: &str, id: &str) -> EvolutionRun {
        self.hub
            .evolution_change(EvolutionChange {
                request_id: "create".into(),
                run_id: id.into(),
                revision: 0,
                action: EvolutionAction::Create {
                    input: EvolutionRunInput {
                        id: id.into(),
                        group_id: "designs".into(),
                        subject_id: "capacity".into(),
                        scenario_id: scenario_id.into(),
                        title: "Improve capacity plan".into(),
                        objective: "Propose and critique a capacity plan".into(),
                        acceptance: "Independent measured score is at least two".into(),
                        assignments: ["proposer-a", "proposer-b", "reviewer"]
                            .into_iter()
                            .map(|person| collaboration_model::AssignmentInput {
                                person_id: person.into(),
                                instruction: format!("Perform the {person} role"),
                            })
                            .collect(),
                        baseline_version_id: "baseline".into(),
                        parameters: json!({}),
                        max_rounds: 2,
                        max_seconds: 60,
                        total_budget_seconds: 120,
                    },
                },
            })
            .await
            .unwrap()
    }

    async fn completed_execution(&self, run: &EvolutionRun) -> collaboration_model::Goal {
        tokio::time::timeout(Duration::from_secs(12), async {
            loop {
                let goal = crate::iteration::current_goal(&self.hub, run)
                    .await
                    .unwrap();
                if goal
                    .work
                    .iter()
                    .all(|work| work.state == WorkState::Submitted)
                {
                    return goal;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn scheduler_reaches_later_iterations_while_earlier_rounds_wait_for_results() {
    let fixture = Fixture::new().await;
    for index in 0..5 {
        fixture
            .create_named("design-review", &format!("waiting-{index}"))
            .await;
    }
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let scheduler = tokio::spawn(crate::iteration::run(
        fixture.hub.clone(),
        crate::default_scenarios(),
        60,
        receiver,
    ));
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let run = fixture.hub.evolution_run("waiting-4").await.unwrap();
            if !run.rounds.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    stop.send(true).unwrap();
    scheduler.await.unwrap();
    for index in 0..5 {
        let run = fixture
            .hub
            .evolution_run(&format!("waiting-{index}"))
            .await
            .unwrap();
        assert_eq!(run.rounds.len(), 1);
        let goal = crate::iteration::current_goal(&fixture.hub, &run)
            .await
            .unwrap();
        assert!(goal.work.iter().all(|work| work.state == WorkState::Ready));
    }
}

#[tokio::test]
async fn non_code_three_agent_iteration_executes_feedback_and_recovers_without_promoting_claims() {
    let fixture = Fixture::new().await;
    let scenarios = crate::default_scenarios();
    let mut run = fixture.create("design-review").await;
    let (stop, _) = tokio::sync::watch::channel(false);
    let mut execution = tokio::spawn(crate::execution::run_with_scenarios(
        fixture.hub.clone(),
        fixture.registry.clone(),
        scenarios.clone(),
        60,
        stop.subscribe(),
    ));
    for round in 1..=2 {
        crate::iteration::advance(&fixture.hub, &scenarios, run.clone())
            .await
            .unwrap();
        // Recovery repeats the identical admission command; there is one goal.
        crate::iteration::advance(&fixture.hub, &scenarios, run.clone())
            .await
            .unwrap();
        run = fixture.hub.evolution_run(&run.input.id).await.unwrap();
        assert_eq!(run.rounds.len(), round);
        let goal = fixture.completed_execution(&run).await;
        assert_eq!(goal.work.len(), 3);
        assert_eq!(
            goal.state,
            GoalState::Active,
            "worker success cannot accept an iteration"
        );
        let candidate = format!("candidate-{round}");
        fixture
            .hub
            .research_change(ResearchChange::Node {
                node: EvolutionInput {
                    id: candidate.clone(),
                    subject_id: run.input.subject_id.clone(),
                    parents: vec![run.baseline_version_id.clone()],
                    version: format!("v{round}"),
                    reference: format!("proposal/v{round}"),
                    goal_id: Some(goal.input.id.clone()),
                    pins: EvidencePins::default(),
                },
            })
            .await
            .unwrap();
        run = fixture
            .change(
                &run,
                EvolutionAction::RecordCandidate {
                    version_id: candidate.clone(),
                    producer_work_id: "work-0".into(),
                },
            )
            .await;
        assert_eq!(
            run.rounds.last().unwrap().candidate.as_ref().unwrap().state,
            CandidateState::Proposed
        );
        let reviewer = goal.work.last().unwrap();
        let evidence = reviewer.result.as_ref().unwrap().evidence.clone();
        let passed = evidence["agent_evidence"]["measured_score"]
            .as_u64()
            .unwrap()
            >= evidence["agent_evidence"]["threshold"].as_u64().unwrap();
        run = fixture
            .change(
                &run,
                EvolutionAction::Evaluate {
                    evaluation: CandidateEvaluation {
                        evaluator_work_id: reviewer.id.clone(),
                        goal_revision: goal.revision,
                        state: if passed {
                            EvaluationState::Passed
                        } else {
                            EvaluationState::Failed
                        },
                        summary: "Measured proposal score against frozen criterion".into(),
                        evidence,
                    },
                },
            )
            .await;
        if passed {
            run = fixture.change(&run, EvolutionAction::Accept).await;
            run = fixture
                .change(
                    &run,
                    EvolutionAction::FinishRound {
                        state: RoundState::Completed,
                        feedback: "Score reaches threshold".into(),
                    },
                )
                .await;
            assert_eq!(run.baseline_version_id, candidate);
            assert_eq!(run.state, IterationState::Completed);
        } else {
            run = fixture
                .change(
                    &run,
                    EvolutionAction::FinishRound {
                        state: RoundState::Failed,
                        feedback: "score below threshold; revise the proposal".into(),
                    },
                )
                .await;
            assert_eq!(run.baseline_version_id, "baseline");
            stop.send_replace(true);
            execution.await.unwrap();
            stop.send_replace(false);
            let registry = Arc::new(service_registry::Registry::open(fixture.root.path()).unwrap());
            execution = tokio::spawn(crate::execution::run_with_scenarios(
                fixture.hub.clone(),
                registry,
                scenarios.clone(),
                60,
                stop.subscribe(),
            ));
        }
    }
    let graph = fixture.hub.graph("plans", "capacity", 0).await.unwrap();
    assert!(
        graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["node"]["input"]["id"] == "candidate-2" && row["status"] == "accepted")
    );
    stop.send_replace(true);
    execution.await.unwrap();
}

#[tokio::test]
async fn unavailable_scenario_cannot_admit_iteration_goal() {
    let fixture = Fixture::new().await;
    let run = fixture.create("uninstalled").await;
    assert!(
        crate::iteration::advance(&fixture.hub, &crate::default_scenarios(), run.clone())
            .await
            .is_err()
    );
    assert!(
        fixture
            .hub
            .evolution_run(&run.input.id)
            .await
            .unwrap()
            .rounds
            .is_empty()
    );
    let page = fixture.hub.goals("designs", "", "", false).await.unwrap();
    assert!(page["goals"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn remote_scenario_protocol_checks_health_goal_digest_current_attempt_and_host_boundary() {
    use std::sync::atomic::{AtomicU8, Ordering};
    let mut fixture = Fixture::new().await;
    let endpoint = fixture.root.path().join("private/services/scenario.sock");
    let listener = service_protocol::Listener::bind(&endpoint).unwrap();
    let mode = Arc::new(AtomicU8::new(0));
    let worker_mode = mode.clone();
    fixture.workers.push(tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.listener.accept().await.unwrap();
            let request: service_protocol::Envelope<service_api::ScenarioRequest> =
                service_protocol::read(&mut stream).await.unwrap();
            let mode = worker_mode.load(Ordering::SeqCst);
            let response = match request.request {
                service_api::ScenarioRequest::Probe => json!(service_api::ScenarioHandshake {
                    protocol_version: if mode == 3 { 99 } else { 1 },
                    id: "remote-design".into(),
                    capabilities: vec![contracts::ScenarioCapability::Execute],
                    ready: true,
                }),
                service_api::ScenarioRequest::Validate { goal } => {
                    json!(service_api::ScenarioValidation {
                        goal_sha256: if mode == 4 {
                            "wrong-goal".into()
                        } else {
                            storage::digest(&serde_json::to_vec(&goal).unwrap())
                        },
                        valid: true,
                    })
                }
                service_api::ScenarioRequest::Reconcile {
                    request_id,
                    run,
                    goal,
                } => json!(service_api::ScenarioReconciliation {
                    request_id: request_id.clone(),
                    run_id: run.input.id.clone(),
                    revision: run.revision,
                    goal_revision: goal.revision + u64::from(mode == 1),
                    goal_attempts: goal
                        .work
                        .iter()
                        .map(|work| (work.id.clone(), work.attempt))
                        .collect(),
                    change: (mode == 2).then_some(EvolutionChange {
                        request_id,
                        run_id: run.input.id,
                        revision: run.revision,
                        action: EvolutionAction::Pause
                    }),
                }),
            };
            service_protocol::write(
                &mut stream,
                &service_protocol::Reply::from_result(Ok(response)),
            )
            .await
            .unwrap();
        }
    }));
    let registration = service_api::ScenarioRegistration {
        id: "remote-design".into(),
        endpoint,
        enabled: true,
        protocol_version: 1,
        capabilities: vec![contracts::ScenarioCapability::Execute],
        dependencies: vec![],
    };
    let mut configuration = fixture.registry.configuration();
    configuration.scenarios.push(registration.clone());
    fixture.registry.save(configuration).unwrap();
    let remote = crate::remote_scenario::RemoteScenario {
        registration,
        dependencies: vec![],
    };
    let scenarios =
        crate::scenarios::configured(&crate::default_scenarios(), Some(&fixture.registry)).unwrap();
    let run = fixture.create("remote-design").await;
    let input = run.next_goal().unwrap();
    remote.admission(&input).await.unwrap();
    mode.store(4, Ordering::SeqCst);
    assert!(remote.admission(&input).await.is_err());
    mode.store(3, Ordering::SeqCst);
    assert!(remote.admission(&input).await.is_err());
    mode.store(0, Ordering::SeqCst);
    crate::iteration::advance(&fixture.hub, &scenarios, run.clone())
        .await
        .unwrap();
    let run = fixture.hub.evolution_run(&run.input.id).await.unwrap();
    remote.reconcile(&fixture.hub, &run).await.unwrap();
    mode.store(1, Ordering::SeqCst);
    assert!(remote.reconcile(&fixture.hub, &run).await.is_err());
    mode.store(2, Ordering::SeqCst);
    assert!(remote.reconcile(&fixture.hub, &run).await.is_err());
    assert_eq!(
        fixture
            .hub
            .evolution_run(&run.input.id)
            .await
            .unwrap()
            .state,
        IterationState::Running
    );
}

#[tokio::test]
async fn iteration_http_api_requires_operator_and_rejects_unavailable_scenario_before_storage() {
    let host = crate::tests::Harness::new().await;
    assert_eq!(
        host.request("/api/im/iterations", None, false, None)
            .await
            .0,
        401
    );
    let (status, body) = host.request("/api/im/iterations", None, true, None).await;
    assert_eq!(status, 200, "{body}");
    assert!(
        serde_json::from_str::<Value>(&body).unwrap()["runs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let change = json!({"request_id":"create","run_id":"run","revision":0,"action":{"operation":"create","input":{
        "id":"run","group_id":"missing-group","subject_id":"missing-subject","scenario_id":"uninstalled",
        "title":"Design","objective":"Compare plans","acceptance":"Measured improvement","assignments":[{"person_id":"worker","instruction":"Propose"}],
        "baseline_version_id":"baseline","parameters":{},"max_rounds":2,"max_seconds":60,"total_budget_seconds":120
    }}});
    let (status, body) = host
        .request("/api/im/iterations", Some(change), true, None)
        .await;
    assert_eq!(status, 400);
    assert!(body.contains("scenario unavailable"), "{body}");
    host.close().await;
}

#[tokio::test]
async fn guarded_scenario_rejects_manual_evidence_and_acceptance_at_the_http_handler() {
    struct Guarded;
    impl ScenarioAdapter for Guarded {
        fn descriptor(&self) -> Scenario {
            let mut descriptor = crate::design_review::DesignReview.descriptor();
            descriptor.manual_review = false;
            descriptor
        }
        fn validate(&self, goal: &collaboration_model::GoalInput) -> Result<()> {
            goal.validate()
        }
    }
    let fixture = Fixture::new().await;
    let run = fixture.create("design-review").await;
    let addr: SocketAddr = "127.0.0.1:8090".parse().unwrap();
    let (stop, _) = tokio::sync::watch::channel(false);
    let mut web = crate::transport::Web::new(fixture.hub.clone(), addr, stop);
    web.scenarios = Arc::new(vec![Arc::new(Guarded)]);
    for action in [
        EvolutionAction::RecordCandidate {
            version_id: "candidate".into(),
            producer_work_id: "work-0".into(),
        },
        EvolutionAction::Evaluate {
            evaluation: CandidateEvaluation {
                evaluator_work_id: "work-2".into(),
                goal_revision: 1,
                state: EvaluationState::Passed,
                summary: "Claim".into(),
                evidence: json!({"claim":true}),
            },
        },
        EvolutionAction::Accept,
        EvolutionAction::BeginActivation {
            claim_id: "manual-claim".into(),
            max_seconds: 60,
        },
        EvolutionAction::Activate {
            activation: collaboration_model::CandidateActivation {
                state: contracts::ActivationState::Active,
                reference: "claimed".into(),
                evidence: json!({"claim":true}),
            },
        },
        EvolutionAction::FinishRound {
            state: RoundState::Completed,
            feedback: "Claim".into(),
        },
    ] {
        let change = EvolutionChange {
            request_id: "manual-bypass".into(),
            run_id: run.input.id.clone(),
            revision: run.revision,
            action,
        };
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/im/iterations")
            .header("host", addr.to_string())
            .header("origin", format!("http://{addr}"))
            .header("content-type", "application/json")
            .header("x-crystal-intent", "manage-crystal")
            .body(axum::body::Body::from(serde_json::to_vec(&change).unwrap()))
            .unwrap();
        let response = crate::iteration::manage(axum::extract::State(web.clone()), request).await;
        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("independent host evidence gate"));
    }
    assert_eq!(
        fixture
            .hub
            .evolution_run(&run.input.id)
            .await
            .unwrap()
            .revision,
        run.revision
    );
}
