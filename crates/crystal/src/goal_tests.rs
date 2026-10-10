use super::*;
fn input() -> GoalInput {
    GoalInput {
        id: "goal".into(),
        group_id: "team".into(),
        title: "Review a proposal".into(),
        objective: "Compare two alternatives".into(),
        acceptance: "Both Agents provide evidence and limitations".into(),
        scenario: "collaboration".into(),
        parameters: json!({}),
        assignments: ["a", "b"]
            .into_iter()
            .map(|person_id| AssignmentInput {
                person_id: person_id.into(),
                instruction: format!("Independent analysis by {person_id}"),
            })
            .collect(),
        max_seconds: 60,
    }
}
async fn setup(hub: &Hub) {
    hub.register_operator().await.unwrap();
    hub.register(vec!["a".into(), "b".into(), "outsider".into()])
        .await
        .unwrap();
    hub.create_group(NewGroup {
        id: "team".into(),
        title: "Team".into(),
        topic: "Discuss work".into(),
        kind: GroupKind::Conversation,
        private: false,
        members: vec!["operator".into(), "a".into(), "b".into()],
    })
    .await
    .unwrap();
}
fn second(mut change: WorkChange) -> WorkChange {
    match &mut change {
        WorkChange::Claim { attempt, .. } | WorkChange::Submit { attempt, .. } => *attempt = 2,
    }
    change
}
fn claim(work: &str, id: &str) -> WorkChange {
    WorkChange::Claim {
        attempt: 1,
        goal_id: "goal".into(),
        work_id: work.into(),
        claim_id: id.into(),
    }
}
fn submit(work: &str, claim: &str, succeeded: bool) -> WorkChange {
    WorkChange::Submit {
        attempt: 1,
        goal_id: "goal".into(),
        work_id: work.into(),
        claim_id: claim.into(),
        result: WorkResult {
            code: None,
            summary: "Findings and limitations recorded".into(),
            succeeded,
            evidence: json!({"artifact":"test-report","sha256":"pinned"}),
        },
    }
}
#[tokio::test]
async fn concurrent_claims_fence_results_and_require_host_acceptance_with_durable_audit() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    let token = hub.grant("a", 3600).await.unwrap();
    let other = hub.grant("b", 3600).await.unwrap();
    let outsider = hub.grant("outsider", 3600).await.unwrap();
    let mut live = hub.subscribe("a", "team").unwrap();
    let goal = hub
        .change_goal(GoalChange::Create { goal: input() })
        .await
        .unwrap();
    let audit = tokio::time::timeout(std::time::Duration::from_secs(1), live.recv())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        audit.event.as_ref().unwrap().kind,
        contracts::GoalEventKind::Created
    );
    let duplicate = hub
        .change_goal(GoalChange::Create { goal: input() })
        .await
        .unwrap();
    assert_eq!(duplicate.revision, goal.revision);
    assert_eq!(hub.history("team", 0, 100).await.unwrap().len(), 1);
    let mut changed = input();
    changed.objective = "Different work".into();
    assert!(
        hub.change_goal(GoalChange::Create { goal: changed })
            .await
            .is_err()
    );
    assert!(
        hub.change_goal(GoalChange::Accept {
            goal_id: "goal".into(),
            revision: 1
        })
        .await
        .is_err()
    );
    assert!(
        hub.work(&outsider, claim("work-0", "outsider"))
            .await
            .is_err()
    );
    assert!(
        hub.work(&other, claim("work-0", "wrong-person"))
            .await
            .is_err()
    );
    assert!(
        hub.assigned_goals(&outsider, "").await.unwrap()["goals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (one, two) = tokio::join!(
        hub.work(&token, claim("work-0", "one")),
        hub.work(&token, claim("work-0", "two"))
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let accepted = if one.is_ok() { "one" } else { "two" };
    assert!(
        hub.work(&token, submit("work-0", "wrong", true))
            .await
            .is_err()
    );
    let first = hub
        .work(&token, submit("work-0", accepted, true))
        .await
        .unwrap();
    assert_eq!(first.state, GoalState::Active);
    assert_eq!(
        hub.work(&token, submit("work-0", accepted, true))
            .await
            .unwrap()
            .revision,
        first.revision
    );
    assert!(hub.membership("team", "a", false).await.is_err());
    assert!(
        hub.change_goal(GoalChange::Accept {
            goal_id: "goal".into(),
            revision: first.revision
        })
        .await
        .is_err()
    );
    hub.work(&other, claim("work-1", "other")).await.unwrap();
    let failed = hub
        .work(&other, submit("work-1", "other", false))
        .await
        .unwrap();
    assert!(
        hub.change_goal(GoalChange::Accept {
            goal_id: "goal".into(),
            revision: failed.revision
        })
        .await
        .is_err()
    );
    let retried = hub
        .change_goal(GoalChange::Retry {
            goal_id: "goal".into(),
            revision: failed.revision,
            work_id: "work-1".into(),
        })
        .await
        .unwrap();
    assert_eq!(retried.work[1].attempt, 2);
    assert!(hub.work(&other, claim("work-1", "other")).await.is_err());
    assert!(
        hub.work(&other, submit("work-1", "other", true))
            .await
            .is_err()
    );
    hub.work(&other, second(claim("work-1", "new-attempt")))
        .await
        .unwrap();
    let submitted = hub
        .work(&other, second(submit("work-1", "new-attempt", true)))
        .await
        .unwrap();
    let completed = hub
        .change_goal(GoalChange::Accept {
            goal_id: "goal".into(),
            revision: submitted.revision,
        })
        .await
        .unwrap();
    assert_eq!(completed.state, GoalState::Completed);
    drop(live);
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    assert_eq!(
        hub.goals("team", "", "", false).await.unwrap()["goals"][0]["state"],
        "completed"
    );
    assert_eq!(
        hub.history("team", 0, 100)
            .await
            .unwrap()
            .last()
            .unwrap()
            .event
            .as_ref()
            .unwrap()
            .kind,
        contracts::GoalEventKind::Accepted
    );

    assert!(hub.work(&other, claim("work-1", "again")).await.is_err());
}
#[tokio::test]
async fn research_results_require_host_adapter_and_archive_cannot_orphan_active_goals() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    let mut goal = input();
    goal.scenario = "research".into();
    hub.change_goal(GoalChange::Create { goal }).await.unwrap();
    let token = hub.grant("a", 60).await.unwrap();
    assert!(
        hub.work(&token, claim("work-0", "forged-evidence"))
            .await
            .is_err()
    );
    let group = hub.group("team").unwrap();
    assert!(
        hub.update_group(GroupChange {
            id: group.id,
            revision: group.revision,
            title: group.title,
            topic: group.topic,
            archived: true,
            pinned: false
        })
        .await
        .is_err()
    );
    let current = hub.work_as("a", claim("work-0", "host")).await.unwrap();
    assert!(
        hub.change_goal(GoalChange::Cancel {
            goal_id: "goal".into(),
            revision: current.revision
        })
        .await
        .is_err()
    );
    assert!(
        hub.work_as("a", claim("work-0", "different-host-claim"))
            .await
            .is_err()
    );
}
#[test]
fn expired_and_revoked_claims_cannot_commit_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("test.sqlite")).unwrap();
    store
        .register(&["operator".into(), "a".into(), "b".into()])
        .unwrap();
    store
        .create_group(&NewGroup {
            id: "team".into(),
            title: "Team".into(),
            topic: "Work".into(),
            kind: GroupKind::Conversation,
            private: false,
            members: vec!["operator".into(), "a".into(), "b".into()],
        })
        .unwrap();
    store
        .goal_command(goals::GoalCommand::Change(GoalChange::Create {
            goal: input(),
        }))
        .unwrap();
    store.grant("a", "hash", now_ms() + 60000).unwrap();
    store
        .goal_work("a", Some("hash"), claim("work-0", "claim"))
        .unwrap();
    let mut goal = goals::read(&store.connection, "goal").unwrap();
    goal.work[0].deadline_ms = Some(now_ms() - 1);
    store
        .connection
        .execute(
            "UPDATE goals SET value=?1 WHERE id='goal'",
            [serde_json::to_string(&goal).unwrap()],
        )
        .unwrap();
    assert!(
        store
            .goal_work("a", Some("hash"), submit("work-0", "claim", true))
            .is_err()
    );
    store
        .goal_command(goals::GoalCommand::Change(GoalChange::Retry {
            goal_id: "goal".into(),
            revision: goal.revision,
            work_id: "work-0".into(),
        }))
        .unwrap();
    store.grant("a", "rotated", now_ms() + 60000).unwrap();
    assert!(
        store
            .goal_work("a", Some("hash"), claim("work-0", "stale-credential"))
            .is_err()
    );
    assert!(
        store
            .goal_work(
                "a",
                Some("rotated"),
                second(claim("work-0", "new-credential"))
            )
            .is_ok()
    );
}
