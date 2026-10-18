use crate::{
    StrategySelections,
    files::read_strict,
    register_prompt_candidate, resolve_registered_prompt, resolve_selection,
    test_support::{example, setup},
};
use std::fs;

#[test]
fn runtime_resolution_rejects_invalid_retained_selection_and_wrong_role() {
    let temp = setup();
    fs::create_dir(temp.path().join("evolution")).unwrap();
    let input = temp.path().join("host/register.json");
    let registry = temp.path().join("evolution/prompts.json");
    storage::write(&input, &example("prompt-register")).unwrap();
    let result = register_prompt_candidate(&input, &registry).unwrap();
    let version = result["version"].as_str().unwrap();
    assert!(resolve_registered_prompt(&registry, version, "research").is_ok());
    assert!(resolve_registered_prompt(&registry, version, "review").is_err());
    assert!(resolve_registered_prompt(&registry, "missing", "research").is_err());
    assert_eq!(resolve_selection(temp.path(), "research").unwrap(), None);
    let selection = temp.path().join("evolution/selections.json");
    let invalid = StrategySelections {
        schema_version: 99,
        ..StrategySelections::default()
    };
    storage::write(&selection, &invalid).unwrap();
    assert!(read_strict::<StrategySelections>(&selection).is_ok());
    assert!(resolve_selection(temp.path(), "research").is_err());
    let before = fs::read(&selection).unwrap();
    assert!(resolve_selection(temp.path(), "unselected-role").is_err());
    assert_eq!(before, fs::read(&selection).unwrap());
}
