use super::*;
use serde_json::json;

#[test]
fn target_prompt_matches_required_tools() {
    let c: Config = serde_json::from_value(json!({
        "schema_version":1,"workspace":"/workspace","state_dir":"/state",
        "superpod":"/superpod","codex":"unused","workflow":"unused",
        "daily_seconds":3600,"max_agents":1,"task_timeout_seconds":20,
        "models":{},"tools":{},"require_latest":false,"skills_manifest":null
    }))
    .unwrap();
    for (tools, desktop, sandbox) in [
        (vec!["computer-use-cli"], true, false),
        (vec!["repo-sandbox"], false, true),
        (vec!["computer-use-cli", "repo-sandbox"], true, true),
        (vec![], false, false),
    ] {
        let task: Task = serde_json::from_value(json!({
            "id":"target-prompt","role":"research","repository":"superpod",
            "prompt":"Observe the requested target.","required_tools":tools
        }))
        .unwrap();
        let prompt = rendered_experiment_prompt(&c, &task, None).unwrap();
        for instruction in [
            "DISPLAY/XAUTHORITY",
            "LAB_DESKTOP_WINDOW",
            "LAB_DESKTOP_TARGET/observed.json",
            "Do not connect to personal desktops",
        ] {
            assert_eq!(
                prompt.contains(instruction),
                desktop,
                "{tools:?}: {instruction}"
            );
        }
        for instruction in [
            "targets-host.json",
            "parent directory of RELAY_MEMORY_HOME",
            "Do not attempt nested sandbox execution",
        ] {
            assert_eq!(
                prompt.contains(instruction),
                sandbox,
                "{tools:?}: {instruction}"
            );
        }
        assert!(prompt.contains("next_tasks must be []"));
    }
}
