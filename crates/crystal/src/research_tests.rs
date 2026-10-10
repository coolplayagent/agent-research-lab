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
        topic: ResearchTopic {
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
        subject: ResearchSubject {
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
                scenario: "collaboration".into(),
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
    hub.research_change(ResearchChange::Node { node: version })
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
                    evidence: json!({"artifact":"report"}),
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
}
#[tokio::test]
async fn research_revisions_pins_and_archival_are_enforced() {
    let (_root, hub) = setup().await;
    let mut topic: ResearchTopic = serde_json::from_value(
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
        subject: ResearchSubject {
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
