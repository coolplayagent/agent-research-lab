use crate::{
    LineageReceipt, LineageRequest, execute,
    files::read_strict,
    test_support::{example, setup},
};
use evolution::PromptRegistry;
use std::collections::BTreeMap;

#[test]
fn lineage_receipt_recomputes_evidence_and_preserves_registry_aliases() {
    let temp = setup();
    let input = temp.path().join("host/register.json");
    let registry = temp.path().join("host/registry.json");
    storage::write(&input, &example("prompt-register")).unwrap();
    execute("prompt-register", &input, Some(&registry)).unwrap();
    let request = temp.path().join("host/lineage.json");
    let study = evolution::lineage::Study {
        task_kind: "long_horizon".into(),
        policy_version: "a".repeat(40),
        superpod_commit: "b".repeat(40),
        expected: BTreeMap::new(),
        observations: vec![],
        pending_evaluations: BTreeMap::new(),
        pending_expansions: BTreeMap::new(),
        evaluation_budget: 10,
    };
    let output = temp.path().join("host/report.json");
    storage::write(
        &request,
        &LineageRequest {
            registry_file: "registry.json".into(),
            study,
        },
    )
    .unwrap();
    execute("lineage", &request, Some(&output)).unwrap();
    let mut receipt: LineageReceipt = storage::read(&output).unwrap();
    receipt.validate().unwrap();
    receipt.report.budget_remaining = 0;
    assert!(receipt.validate().is_err());
    let alias = temp.path().join("host/alias.json");
    std::os::unix::fs::symlink(&registry, &alias).unwrap();
    assert!(execute("lineage", &request, Some(&alias)).is_err());
    read_strict::<PromptRegistry>(&registry)
        .unwrap()
        .validate()
        .unwrap();
}
