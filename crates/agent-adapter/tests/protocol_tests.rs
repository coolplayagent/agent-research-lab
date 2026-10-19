use anyhow::{Context, Result, ensure};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
fn binary(name: &str, cargo: Option<&str>) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(cargo.expect("fixture binary must be built by Cargo or Bazel"))
        })
}
#[test]
fn all_four_bridges_consume_real_stdio_and_preserve_request_identity() -> Result<()> {
    let bridge = std::fs::canonicalize(binary(
        "OUTCOME_TEST_BRIDGE",
        option_env!("CARGO_BIN_EXE_outcome-hub-agent"),
    ))?;
    let fixture = std::fs::canonicalize(binary(
        "OUTCOME_TEST_FIXTURE",
        option_env!("CARGO_BIN_EXE_outcome-hub-agent-fixture"),
    ))?;
    let digest = agent_adapter::executable_digest(&fixture, &[])?;
    for agent in ["codex", "cc", "pi", "hds"] {
        let root = tempfile::tempdir()?;
        std::fs::write(
            root.path().join("binding-input.txt"),
            b"host-verifies-this\n",
        )?;
        let mut command = Command::new(&bridge);
        command
            .args(["--agent", agent, "--program"])
            .arg(&fixture)
            .args(["--digest", &digest, "--seconds", "10"]);
        if agent == "hds" {
            command.args(["--model", "fixture/deterministic"]);
        }
        let mut child = command
            .current_dir(root.path())
            .env("OUTCOME_HUB_ISOLATED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().context("stdin missing")?.write_all(serde_json::json!({"protocol_version":1,"request_id":"fenced-attempt","person_id":"worker","goal_id":"goal","work_id":"work","attempt":3,"objective":"fixture","acceptance":"fixture","instruction":"fixture","messages":[]}).to_string().as_bytes())?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "bridge {agent} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: service_api::AgentOutput = serde_json::from_slice(&output.stdout)?;
        assert!(response.succeeded);
        assert_eq!(response.request_id, "fenced-attempt");
        assert_eq!(
            std::fs::read(root.path().join("binding-result.txt"))?,
            b"host-verifies-this\n"
        );
    }
    Ok(())
}
