use super::*;
fn campaign() -> Campaign {
    Campaign {
        schema_version: 1,
        id: "fixture".into(),
        group_id: "group".into(),
        person_ids: ["a".into(), "b".into(), "c".into(), "d".into()],
        repositories: ["one".into(), "two".into(), "three".into(), "four".into()],
        objective: "Inspect CLI failure recovery".into(),
        acceptance: "Cite pinned evidence".into(),
        instructions: std::array::from_fn(|_| "Inspect independently".into()),
        max_seconds: 60,
        pause_seconds: 10,
        failure_backoff_seconds: 120,
        max_rounds: 1_000_000,
    }
}
fn completed(c: &Campaign) -> Goal {
    Goal {
        input: c.goal(0),
        state: GoalState::Active,
        revision: 5,
        created_ms: 1000,
        work: (0..4)
            .map(|i| collaboration_model::WorkItem {
                id: format!("work-{i}"),
                person_id: c.person_ids[i].clone(),
                instruction: c.instructions[i].clone(),
                state: WorkState::Submitted,
                attempt: 1,
                claim_id: Some("claim".into()),
                deadline_ms: Some(61000),
                result: None,
            })
            .collect(),
    }
}
#[test]
fn restart_rebuilds_cursor_and_rotates_four_parallel_members() {
    let c = campaign();
    let goal = completed(&c);
    assert_eq!(c.next_round(&[], 0).unwrap(), Some(0));
    assert_eq!(
        c.next_round(std::slice::from_ref(&goal), 71000).unwrap(),
        Some(1)
    );
    assert_eq!(c.next_round(&[goal], 71000).unwrap(), Some(1));
    assert_eq!(c.goal(1).parameters["repository"], "two");
    assert_eq!(c.goal(1).parameters["interaction"], "parallel");
    assert_eq!(c.goal(1).assignments.len(), 4);
}
#[test]
fn running_failure_and_cancellation_block_early_rounds() {
    let c = campaign();
    let mut goal = completed(&c);
    goal.work[3].state = WorkState::Running;
    assert_eq!(
        c.next_round(std::slice::from_ref(&goal), 60999).unwrap(),
        None
    );
    goal.work[3].state = WorkState::Failed;
    assert_eq!(
        c.next_round(std::slice::from_ref(&goal), 180999).unwrap(),
        None
    );
    assert_eq!(
        c.next_round(std::slice::from_ref(&goal), 181000).unwrap(),
        Some(1)
    );
    goal.state = GoalState::Cancelled;
    assert_eq!(c.next_round(&[goal], 999999).unwrap(), None);
}
#[test]
fn edited_config_or_missing_members_cannot_restart_campaign() {
    let mut c = campaign();
    let goal = completed(&c);
    c.failure_backoff_seconds += 1;
    assert!(c.next_round(&[goal], 999999).is_err());
    let c = campaign();
    let mut goal = completed(&c);
    goal.work.pop();
    assert_eq!(c.next_round(&[goal], 999999).unwrap(), None);
    let mut c = campaign();
    c.max_rounds = 1;
    assert_eq!(c.next_round(&[completed(&c)], 999999).unwrap(), None);
}
#[test]
fn complete_policy_is_prompt_bound_and_invalid_digest_is_rejected() {
    let c = campaign();
    let mut goal = completed(&c);
    let task = super::super::research_scenario::task(&goal, &goal.work[0], &[]).unwrap();
    assert!(task.prompt.contains(&c.policy_digest()));
    for field in ["pause_seconds", "failure_backoff_seconds", "max_rounds"] {
        let mut changed = serde_json::to_value(&c).unwrap();
        changed[field] = json!(changed[field].as_u64().unwrap() + 1);
        let changed: Campaign = serde_json::from_value(changed).unwrap();
        assert!(
            changed
                .next_round(std::slice::from_ref(&goal), 999999)
                .is_err()
        );
    }
    goal.input.parameters["campaign_policy_digest"] = json!("invalid");
    assert!(super::super::research_scenario::task(&goal, &goal.work[0], &[]).is_err());
    goal.input
        .parameters
        .as_object_mut()
        .unwrap()
        .remove("campaign_policy_digest");
    assert!(super::super::research_scenario::task(&goal, &goal.work[0], &[]).is_ok());
}
#[tokio::test]
async fn storage_admission_is_durable_idempotent_and_membership_bound() {
    let root = std::env::temp_dir().join(format!(
        "research-campaign-{}-{}",
        std::process::id(),
        collaboration_model::now_ms()
    ));
    fs::create_dir_all(&root).unwrap();
    let c: Config = serde_json::from_value(json!({"schema_version":1,"workspace":root,"state_dir":root.join("state"),"superpod":root,"codex":"unused","workflow":"unused","daily_seconds":600,"max_agents":4,"task_timeout_seconds":60,"require_latest":false,"models":{"research":"fixture"},"tools":{"one":{"binary":"unused","repository":root,"probe":[]},"two":{"binary":"unused","repository":root,"probe":[]},"three":{"binary":"unused","repository":root,"probe":[]},"four":{"binary":"unused","repository":root,"probe":[]}}})).unwrap();
    let mut campaign = campaign();
    campaign.max_seconds = 120;
    for i in 0..4 {
        let person = runtime::people::change(
            &c,
            runtime::people::Change::Create {
                name: format!("Researcher{i}"),
                role: "research".into(),
                kind: runtime::people::Kind::Research,
                soul: "Inspect evidence".into(),
                purpose: "CLI research".into(),
            },
        )
        .unwrap();
        campaign.person_ids[i] = person.id;
    }
    let c = Arc::new(c);
    let store = root.join("collaboration");
    let hub =
        collaboration_storage_service::client(collaboration_store::Hub::open(&store).unwrap());
    hub.register_operator().await.unwrap();
    hub.register(campaign.person_ids.to_vec()).await.unwrap();
    hub.create_group(collaboration_model::NewGroup {
        id: campaign.group_id.clone(),
        title: "Research".into(),
        topic: "CLI".into(),
        private: false,
        kind: collaboration_model::GroupKind::Conversation,
        members: vec![
            "operator".into(),
            campaign.person_ids[0].clone(),
            campaign.person_ids[1].clone(),
            campaign.person_ids[2].clone(),
        ],
    })
    .await
    .unwrap();
    assert!(admit_campaign(&hub, c.clone(), &campaign, 0).await.is_err());
    assert!(
        hub.goals(&campaign.group_id, "research", "", false)
            .await
            .unwrap()["goals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    hub.membership(&campaign.group_id, &campaign.person_ids[3], true)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        admit_campaign(&hub, c.clone(), &campaign, 0),
        admit_campaign(&hub, c.clone(), &campaign, 0)
    );
    a.unwrap();
    b.unwrap();
    let page = hub
        .goals(&campaign.group_id, "research", "", false)
        .await
        .unwrap();
    let goals: Vec<Goal> = serde_json::from_value(page["goals"].clone()).unwrap();
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0].work.len(), 4);
    assert!(
        goals[0]
            .work
            .iter()
            .all(|w| super::super::research_scenario::turn_ready(&goals[0], w).unwrap())
    );
    let path = c.state_dir.join("private/research-campaign.json");
    storage::write(&path, &campaign).unwrap();
    admit(&hub, c.clone()).await.unwrap();
    let mut edited = campaign.clone();
    edited.objective = "Different objective".into();
    assert!(admit_campaign(&hub, c.clone(), &edited, 0).await.is_err());
    fs::remove_file(&path).unwrap();
    admit(&hub, c.clone()).await.unwrap();
    let mut claimed = goals[0].clone();
    for work in &goals[0].work {
        claimed = hub
            .work_as(
                &work.person_id,
                collaboration_model::WorkChange::Claim {
                    attempt: work.attempt,
                    goal_id: goals[0].input.id.clone(),
                    work_id: work.id.clone(),
                    claim_id: format!("claim-{}", work.id),
                },
            )
            .await
            .unwrap();
    }
    let deadline = claimed
        .work
        .iter()
        .filter_map(|w| w.deadline_ms)
        .max()
        .unwrap();
    admit_campaign(&hub, c.clone(), &campaign, deadline - 1)
        .await
        .unwrap();
    admit_campaign(&hub, c.clone(), &campaign, deadline + 119999)
        .await
        .unwrap();
    assert_eq!(
        hub.goals(&campaign.group_id, "research", "", false)
            .await
            .unwrap()["goals"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    admit_campaign(&hub, c.clone(), &campaign, deadline + 120000)
        .await
        .unwrap();
    assert_eq!(
        hub.goals(&campaign.group_id, "research", "", false)
            .await
            .unwrap()["goals"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    hub.change_goal(GoalChange::Cancel {
        goal_id: campaign.goal(1).id,
        revision: 1,
    })
    .await
    .unwrap();
    drop(hub);
    let hub =
        collaboration_storage_service::client(collaboration_store::Hub::open(&store).unwrap());
    admit_campaign(&hub, c, &campaign, u64::MAX).await.unwrap();
    assert_eq!(
        hub.goals(&campaign.group_id, "research", "", false)
            .await
            .unwrap()["goals"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    drop(hub);
    fs::remove_dir_all(root).unwrap();
}
