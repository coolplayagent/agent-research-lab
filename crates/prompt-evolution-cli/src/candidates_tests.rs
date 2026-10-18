use crate::{
    execute,
    files::read_strict,
    test_support::{example, setup},
};
use evolution::PromptRegistry;
use serde_json::Value;

#[test]
fn example_prompt_registration_is_idempotent_and_tampering_fails() {
    let temp = setup();
    let input = temp.path().join("host/request.json");
    let output = temp.path().join("host/registry.json");
    storage::write(&input, &example("prompt-register")).unwrap();
    assert!(execute("prompt-register", &input, None).is_err());
    let result = execute("prompt-register", &input, Some(&output)).unwrap();
    assert_eq!(result["already_registered"], false);
    assert_eq!(
        execute("prompt-register", &input, Some(&output)).unwrap()["already_registered"],
        true
    );
    let registry: PromptRegistry = read_strict(&output).unwrap();
    registry.validate().unwrap();
    let version = result["version"].as_str().unwrap();
    let mut changed: Value = storage::read(&output).unwrap();
    changed["versions"][version]["definition"]["content"] = "tampered".into();
    storage::write(&output, &changed).unwrap();
    assert!(execute("prompt-register", &input, Some(&output)).is_err());
}

#[test]
fn rule_example_stays_quarantined() {
    let temp = setup();
    let input = temp.path().join("host/rule.json");
    storage::write(&input, &example("rule-proposal")).unwrap();
    let result = execute("rule-proposal", &input, None).unwrap();
    assert_eq!(result["quarantined"], true);
    assert_eq!(result["adopted"], false);
}
