use super::*;

#[test]
fn current_goal_orders_activity_creation_and_id_without_promoting_submissions() {
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
    let current = |store: &mut Store| {
        store
            .goal_command(goals::GoalCommand::Current {
                group: "team".into(),
            })
            .unwrap()
    };
    assert_eq!(current(&mut store), json!({"goals":[],"next_after":null}));
    let base = now_ms();
    for (id, created, state, work_state, deadline) in [
        (
            "a-submitted",
            700,
            GoalState::Active,
            WorkState::Submitted,
            None,
        ),
        (
            "b-expired",
            600,
            GoalState::Active,
            WorkState::Running,
            Some(base - 1),
        ),
        ("c-null", 500, GoalState::Active, WorkState::Running, None),
        (
            "d-cancelled",
            900,
            GoalState::Cancelled,
            WorkState::Running,
            Some(base + 60000),
        ),
        (
            "e-completed",
            800,
            GoalState::Completed,
            WorkState::Submitted,
            None,
        ),
        ("f-ready", 400, GoalState::Active, WorkState::Ready, None),
        (
            "g-older-running",
            100,
            GoalState::Active,
            WorkState::Running,
            Some(base + 60000),
        ),
        (
            "y-running",
            200,
            GoalState::Active,
            WorkState::Running,
            Some(base + 60000),
        ),
        (
            "z-running",
            200,
            GoalState::Active,
            WorkState::Running,
            Some(base + 60000),
        ),
    ] {
        let mut goal_input = input();
        goal_input.id = id.into();
        store
            .goal_command(goals::GoalCommand::Change(GoalChange::Create {
                goal: goal_input,
            }))
            .unwrap();
        let mut goal = goals::read(&store.connection, id).unwrap();
        goal.created_ms = created;
        goal.state = state;
        for work in &mut goal.work {
            work.state = work_state;
            work.deadline_ms = deadline;
        }
        store
            .connection
            .execute(
                "UPDATE goals SET value=?2 WHERE id=?1",
                rusqlite::params![id, serde_json::to_string(&goal).unwrap()],
            )
            .unwrap();
    }
    for expected in [
        "y-running",
        "z-running",
        "g-older-running",
        "f-ready",
        "a-submitted",
    ] {
        let page = current(&mut store);
        assert_eq!(page["goals"].as_array().unwrap().len(), 1);
        assert_eq!(page["goals"][0]["input"]["id"], expected);
        assert_eq!(page["goals"][0]["state"], GoalState::Active.as_str());
        assert!(page["next_after"].is_null());
        store
            .connection
            .execute("DELETE FROM goal_assignees WHERE goal_id=?1", [expected])
            .unwrap();
        store
            .connection
            .execute("DELETE FROM goals WHERE id=?1", [expected])
            .unwrap();
    }
    assert!(
        store
            .goal_command(goals::GoalCommand::Current { group: "".into() })
            .is_err()
    );
    assert!(
        store
            .goal_command(goals::GoalCommand::Current {
                group: "absent".into()
            })
            .is_err()
    );
    store
        .connection
        .execute(
            "UPDATE goals SET value=json_set(value,'$.state','future-state','$.created_ms',1000)",
            [],
        )
        .unwrap();
    assert!(
        store
            .goal_command(goals::GoalCommand::Current {
                group: "team".into()
            })
            .is_err()
    );
}

#[tokio::test]
async fn current_goal_finds_work_beyond_history_page_without_writing_or_changing_cursors() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    for index in 0..51 {
        let mut goal = input();
        goal.id = format!("goal-{index:02}");
        hub.change_goal(GoalChange::Create { goal }).await.unwrap();
    }
    let mut change = claim("work-0", "current");
    if let WorkChange::Claim { goal_id, .. } = &mut change {
        *goal_id = "goal-50".into();
    }
    hub.work_as("a", change).await.unwrap();
    let first = hub.goals("team", "", "", false).await.unwrap();
    assert_eq!(first["goals"].as_array().unwrap().len(), 50);
    assert_eq!(first["next_after"], "goal-49");
    let before = hub.metrics();
    let selected = hub.current_goal("team").await.unwrap();
    assert_eq!(selected["goals"][0]["input"]["id"], "goal-50");
    assert!(selected["next_after"].is_null());
    assert_eq!(
        hub.metrics()["catalog_revision"],
        before["catalog_revision"]
    );
    assert_eq!(hub.metrics()["stored_messages"], before["stored_messages"]);
    assert_eq!(hub.goals("team", "", "", false).await.unwrap(), first);
    assert_eq!(
        hub.goals("team", "", "goal-49", false).await.unwrap()["goals"][0]["input"]["id"],
        "goal-50"
    );
}

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

#[tokio::test]
async fn submitted_contributions_are_durable_person_authored_replies_and_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    hub.change_goal(GoalChange::Create { goal: input() })
        .await
        .unwrap();
    hub.work_as("a", claim("work-0", "chat")).await.unwrap();
    hub.work_as("a", submit("work-0", "chat", true))
        .await
        .unwrap();
    hub.work_as("a", submit("work-0", "chat", true))
        .await
        .unwrap();
    let messages = hub.history("team", 0, 100).await.unwrap();
    assert_eq!(messages.len(), 3);
    let reply = messages.last().unwrap();
    assert_eq!(reply.sender_id, "a");
    assert_eq!(reply.text, "Findings and limitations recorded");
    assert_eq!(reply.reply_to, Some(messages[0].sequence));
    assert_eq!(reply.event.as_ref().unwrap().attempt, Some(1));
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    assert_eq!(
        hub.history("team", 0, 100)
            .await
            .unwrap()
            .last()
            .unwrap()
            .text,
        reply.text
    );
}

#[tokio::test]
async fn mentioned_message_and_goal_commit_atomically_and_replay_once() {
    let dir = tempfile::tempdir().unwrap();
    let hub = Hub::open(dir.path()).unwrap();
    setup(&hub).await;
    let message = Publish {
        group_id: "team".into(),
        request_id: "mention-request".into(),
        text: "@a @b Compare alternatives".into(),
        reply_to: None,
    };
    let mut goal = input();
    goal.id = format!("mention-{}", storage::digest(b"team:mention-request"));
    goal.objective = message.text.clone();
    let mut invalid = goal.clone();
    invalid.assignments[1].person_id = "outsider".into();
    assert!(hub.discuss(invalid, message.clone()).await.is_err());
    assert!(hub.history("team", 0, 100).await.unwrap().is_empty());
    let first = hub.discuss(goal.clone(), message.clone()).await.unwrap();
    let again = hub.discuss(goal.clone(), message.clone()).await.unwrap();
    assert_eq!(first["message"]["sequence"], again["message"]["sequence"]);
    assert_eq!(again["duplicate"], true);
    assert_eq!(hub.history("team", 0, 100).await.unwrap().len(), 2);
    assert_eq!(
        hub.goals("team", "", "", false).await.unwrap()["goals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut changed = message;
    changed.text = "@a A different question".into();
    goal.objective = changed.text.clone();
    assert!(hub.discuss(goal, changed).await.is_err());
    let audit = hub.history("team", 0, 100).await.unwrap();
    assert_eq!(audit[1].reply_to, Some(audit[0].sequence));
    drop(hub);
    let hub = Hub::open(dir.path()).unwrap();
    assert_eq!(hub.history("team", 0, 100).await.unwrap().len(), 2);
}
