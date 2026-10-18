use super::*;
use crate::tests::fixture;

#[test]
fn two_arms_have_matched_specialists_and_only_the_planned_peer_edges() {
    let (_temp, c, plan, _) = fixture();
    validate_plan(&c, &plan).unwrap();
    assert_eq!(plan.tasks.len(), 32);
    for i in 0..8 {
        let a = &plan.tasks[16 + i];
        let b = &plan.tasks[24 + i];
        assert_eq!(a.task.dependencies, vec![plan.tasks[i].task.id.clone()]);
        assert_eq!(
            b.task.dependencies,
            vec![
                plan.tasks[8 + i].task.id.clone(),
                plan.tasks[8 + (i + 1) % 8].task.id.clone(),
                plan.tasks[8 + (i + 2) % 8].task.id.clone()
            ]
        );
        assert_eq!(a.model, b.model);
        assert_eq!(a.task.prompt_version, b.task.prompt_version);
        assert_eq!(a.task.max_attempts, Some(1));
        assert_eq!(b.task.max_attempts, Some(1));
        assert!(!a.task.write && !a.task.use_memory && !b.task.write && !b.task.use_memory);
    }
    let mut tampered = plan.clone();
    tampered.tasks[24]
        .task
        .dependencies
        .push(plan.tasks[0].task.id.clone());
    tampered.digest = plan_digest(&tampered).unwrap();
    assert!(validate_plan(&c, &tampered).is_err());
    let mut request = plan.request.clone();
    request.slots[0].model = "different".into();
    assert!(validate_request(&c, &request).is_err());
}
