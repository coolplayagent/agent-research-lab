use super::*;
use contracts::{EvolutionState, SubjectKind, TopicState};
async fn setup() -> (tempfile::TempDir, Hub) {
    let root = tempfile::tempdir().unwrap();
    let hub = Hub::open(root.path()).unwrap();
    hub.register_operator().await.unwrap();
    hub.register(vec!["worker".into()]).await.unwrap();
    hub.create_group(NewGroup {
        id: "group".into(),
        title: "Research".into(),
        topic: "Research coordination".into(),
        kind: GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), "worker".into()],
    })
    .await
    .unwrap();
    hub.research_change(ResearchChange::Topic {
        topic: CollaborationTopic {
            id: "topic".into(),
            group_id: "group".into(),
            title: "Question".into(),
            objective: "Compare versions".into(),
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
            name: "Tool".into(),
            kind: SubjectKind::Tool,
            reference: "repository/tool".into(),
            revision: 0,
        },
    })
    .await
    .unwrap();
    (root, hub)
}
fn input(id: &str, parents: Vec<String>) -> EvolutionInput {
    EvolutionInput {
        id: id.into(),
        subject_id: "subject".into(),
        parents,
        version: id.into(),
        reference: format!("artifact/{id}"),
        goal_id: None,
        pins: EvidencePins::default(),
    }
}
#[tokio::test]
async fn lineage_is_durable_append_only_and_acyclic() {
    let (root, hub) = setup().await;
    hub.research_change(ResearchChange::Node {
        node: input("base", vec![]),
    })
    .await
    .unwrap();
    for name in ["left", "right"] {
        hub.research_change(ResearchChange::Node {
            node: input(name, vec!["base".into()]),
        })
        .await
        .unwrap();
    }
    hub.research_change(ResearchChange::Node {
        node: input("child", vec!["left".into(), "right".into()]),
    })
    .await
    .unwrap();
    assert!(
        hub.research_change(ResearchChange::Node {
            node: input("base", vec!["child".into()])
        })
        .await
        .is_err()
    );
    assert!(
        hub.research_change(ResearchChange::Node {
            node: input("missing", vec!["absent".into()])
        })
        .await
        .is_err()
    );
    assert!(
        hub.research_change(ResearchChange::Node {
            node: input("duplicate", vec!["base".into(), "base".into()])
        })
        .await
        .is_err()
    );
    drop(hub);
    let hub = Hub::open(root.path()).unwrap();
    let graph = hub.graph("topic", "subject", 0).await.unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 4);
    assert_eq!(graph["nodes"][3]["node"]["generation"], 2);
    assert_eq!(graph["nodes"][3]["operation"], "recombination");
    assert_eq!(
        graph["nodes"][3]["status"],
        EvolutionState::Unassessed.as_str()
    );
}
#[tokio::test]
async fn graph_verdict_comes_from_host_accepted_goal() {
    let (_root, hub) = setup().await;
    let goal = hub
        .change_goal(GoalChange::Create {
            goal: GoalInput {
                id: "goal".into(),
                group_id: "group".into(),
                title: "Evaluate".into(),
                objective: "Compare tool".into(),
                acceptance: "Host reviews result".into(),
                scenario: "research".into(),
                parameters: json!({}),
                assignments: vec![AssignmentInput {
                    person_id: "worker".into(),
                    instruction: "Evaluate version".into(),
                }],
                max_seconds: 60,
            },
        })
        .await
        .unwrap();
    let mut version = input("v1", vec![]);
    version.goal_id = Some("goal".into());
    version.pins = EvidencePins {
        source: Some("a".repeat(40)),
        prompt: Some("b".repeat(64)),
        policy: Some("c".repeat(64)),
        superpod: Some("d".repeat(40)),
    };
    let mut unpinned = version.clone();
    unpinned.id = "unpinned".into();
    unpinned.pins = EvidencePins::default();
    let mut mismatch = version.clone();
    mismatch.id = "mismatch".into();
    mismatch.pins.policy = Some("e".repeat(64));
    hub.research_change(ResearchChange::Node { node: version })
        .await
        .unwrap();
    hub.research_change(ResearchChange::Node { node: unpinned })
        .await
        .unwrap();
    hub.research_change(ResearchChange::Node { node: mismatch })
        .await
        .unwrap();
    let work = &goal.work[0];
    hub.work_as(
        "worker",
        WorkChange::Claim {
            goal_id: "goal".into(),
            work_id: work.id.clone(),
            claim_id: "claim".into(),
            attempt: work.attempt,
        },
    )
    .await
    .unwrap();
    let goal = hub
        .work_as(
            "worker",
            WorkChange::Submit {
                goal_id: "goal".into(),
                work_id: work.id.clone(),
                claim_id: "claim".into(),
                attempt: work.attempt,
                result: WorkResult {
                    code: None,
                    summary: "Evidence ready".into(),
                    succeeded: true,
                    evidence: json!({"receipt_protocol":2,"receipt_sha256":"f".repeat(64),"source_commit":"a".repeat(40),"prompt_digest":"b".repeat(64),"policy_digest":"c".repeat(64),"superpod_commit":"d".repeat(40)}),
                },
            },
        )
        .await
        .unwrap();
    assert_eq!(
        hub.graph("", "", 0).await.unwrap()["nodes"][0]["status"],
        EvolutionState::Submitted.as_str()
    );
    hub.change_goal(GoalChange::Accept {
        goal_id: "goal".into(),
        revision: goal.revision,
    })
    .await
    .unwrap();
    assert_eq!(
        hub.graph("", "", 0).await.unwrap()["nodes"][0]["status"],
        EvolutionState::Accepted.as_str()
    );
    let graph = hub.graph("", "", 0).await.unwrap();
    for row in &graph["nodes"].as_array().unwrap()[1..] {
        assert_eq!(row["status"], EvolutionState::Unassessed.as_str());
    }
    let node: EvolutionNode = serde_json::from_value(graph["nodes"][0]["node"].clone()).unwrap();
    let accepted: Goal = serde_json::from_value(graph["nodes"][0]["goal"].clone()).unwrap();
    let mut untrusted = accepted.clone();
    untrusted.input.scenario = "collaboration".into();
    assert_eq!(
        coordination::assessment(&node, Some(&untrusted)),
        EvolutionState::Unassessed
    );
    let mut retried = accepted.clone();
    retried.work[0].attempt += 1;
    assert_eq!(
        coordination::assessment(&node, Some(&retried)),
        EvolutionState::Unassessed
    );
    for field in [
        "source_commit",
        "prompt_digest",
        "policy_digest",
        "superpod_commit",
        "receipt_protocol",
        "receipt_sha256",
    ] {
        let mut changed = accepted.clone();
        changed.work[0].result.as_mut().unwrap().evidence[field] = Value::Null;
        assert_eq!(
            coordination::assessment(&node, Some(&changed)),
            EvolutionState::Unassessed,
            "{field}"
        );
    }
    let mut historical = serde_json::to_value(&node).unwrap();
    historical.as_object_mut().unwrap().remove("goal_attempts");
    let historical: EvolutionNode = serde_json::from_value(historical).unwrap();
    assert_eq!(
        coordination::assessment(&historical, Some(&accepted)),
        EvolutionState::Unassessed
    );
}
#[tokio::test]
async fn research_revisions_pins_and_archival_are_enforced() {
    let (_root, hub) = setup().await;
    let mut topic: CollaborationTopic = serde_json::from_value(
        hub.topics("", "Question").await.unwrap()["topics"][0]["topic"].clone(),
    )
    .unwrap();
    let mut stale = topic.clone();
    stale.revision = 0;
    assert!(
        hub.research_change(ResearchChange::Topic { topic: stale })
            .await
            .is_err()
    );
    let mut bad = input("bad", vec![]);
    bad.pins.source = Some("partial".into());
    assert!(
        hub.research_change(ResearchChange::Node { node: bad })
            .await
            .is_err()
    );
    topic.state = TopicState::Archived;
    hub.research_change(ResearchChange::Topic { topic })
        .await
        .unwrap();
    assert!(
        hub.research_change(ResearchChange::Node {
            node: input("later", vec![])
        })
        .await
        .is_err()
    );
    assert!(
        hub.graph("", "", 0).await.unwrap()["nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn graph_pagination_preserves_parent_order_and_rejects_cross_subject_edges() {
    let (_root, hub) = setup().await;
    hub.research_change(ResearchChange::Subject {
        subject: CollaborationSubject {
            id: "other".into(),
            topic_id: "topic".into(),
            name: "Another tool".into(),
            kind: SubjectKind::Tool,
            reference: "repository/other".into(),
            revision: 0,
        },
    })
    .await
    .unwrap();
    for index in 0..130 {
        hub.research_change(ResearchChange::Node {
            node: input(
                &format!("version-{index}"),
                if index == 0 {
                    vec![]
                } else {
                    vec![format!("version-{}", index - 1)]
                },
            ),
        })
        .await
        .unwrap();
    }
    let first = hub.graph("topic", "subject", 0).await.unwrap();
    assert_eq!(first["nodes"].as_array().unwrap().len(), 128);
    let second = hub
        .graph("topic", "subject", first["next_after"].as_u64().unwrap())
        .await
        .unwrap();
    assert_eq!(second["nodes"].as_array().unwrap().len(), 2);
    assert!(second["next_after"].is_null());
    assert_eq!(second["nodes"][0]["node"]["generation"], 128);
    let mut invalid = input("cross-subject", vec!["version-0".into()]);
    invalid.subject_id = "other".into();
    assert!(
        hub.research_change(ResearchChange::Node { node: invalid })
            .await
            .is_err()
    );
    let mut invalid = serde_json::to_value(input("forged", vec![])).unwrap();
    invalid["status"] = json!(EvolutionState::Accepted);
    assert!(serde_json::from_value::<EvolutionInput>(invalid).is_err());
}

#[tokio::test]
async fn repeated_assessments_keep_one_version_and_fence_attempts() {
    let (_root, hub) = setup().await;
    let mut version = input("baseline", vec![]);
    version.pins.source = Some("a".repeat(40));
    hub.research_change(ResearchChange::Node { node: version })
        .await
        .unwrap();
    for name in ["first", "second"] {
        let goal = hub
            .change_goal(GoalChange::Create {
                goal: GoalInput {
                    id: name.into(),
                    group_id: "group".into(),
                    title: "Assess existing version".into(),
                    objective: "Compare observations".into(),
                    acceptance: "Bound independent evidence".into(),
                    scenario: "design-review".into(),
                    parameters: json!({}),
                    assignments: vec![AssignmentInput {
                        person_id: "worker".into(),
                        instruction: "Evaluate".into(),
                    }],
                    max_seconds: 60,
                },
            })
            .await
            .unwrap();
        let work = &goal.work[0];
        let pins = EvidencePins {
            source: Some("a".repeat(40)),
            prompt: Some("b".repeat(64)),
            policy: Some("c".repeat(64)),
            superpod: Some("d".repeat(40)),
        };
        let assessment = AssessmentInput {
            id: format!("assessment-{name}"),
            node_id: "baseline".into(),
            goal_id: name.into(),
            goal_attempts: [(work.id.clone(), work.attempt)].into(),
            pins_by_work: [(work.id.clone(), pins)].into(),
        };
        for _ in 0..2 {
            hub.research_change(ResearchChange::Assessment {
                assessment: assessment.clone(),
            })
            .await
            .unwrap();
        }
        hub.work_as(
            "worker",
            WorkChange::Claim {
                goal_id: name.into(),
                work_id: work.id.clone(),
                claim_id: "claim".into(),
                attempt: work.attempt,
            },
        )
        .await
        .unwrap();
        let submitted = hub.work_as("worker", WorkChange::Submit { goal_id:name.into(),work_id:work.id.clone(),claim_id:"claim".into(),attempt:work.attempt,
            result: WorkResult { code:None,summary:"Evidence".into(),succeeded:true,evidence:json!({"source_commit":"a".repeat(40),"prompt_digest":"b".repeat(64),"policy_digest":"c".repeat(64),"superpod_commit":"d".repeat(40)}) }}).await.unwrap();
        assert_eq!(
            hub.graph("topic", "subject", 0).await.unwrap()["nodes"][0]["status"],
            EvolutionState::Submitted.as_str()
        );
        hub.change_goal(GoalChange::Accept {
            goal_id: name.into(),
            revision: submitted.revision,
        })
        .await
        .unwrap();
        assert_eq!(
            hub.graph("topic", "subject", 0).await.unwrap()["nodes"][0]["status"],
            EvolutionState::Accepted.as_str()
        );
        let mut changed = assessment.clone();
        changed.pins_by_work.get_mut(&work.id).unwrap().source = Some("f".repeat(40));
        assert!(
            hub.research_change(ResearchChange::Assessment {
                assessment: changed
            })
            .await
            .is_err()
        );
        let mut stale = goal.clone();
        stale.work[0].attempt += 1;
        assert_eq!(
            coordination::version_assessment(
                &VersionAssessment {
                    input: assessment,
                    created_ms: 0
                },
                &stale
            ),
            EvolutionState::Unassessed
        );
    }
    let graph = hub.graph("topic", "subject", 0).await.unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(graph["nodes"][0]["assessment_count"], 2);
}
