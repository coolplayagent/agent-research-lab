//! One explicit Codex permission policy for every controller-owned agent.
//! The outer task namespace still keeps host credentials and receipts private.
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub const APPROVAL_POLICY: &str = "never";
pub const SANDBOX_MODE: &str = "danger-full-access";

pub fn arguments() -> Vec<String> {
    [
        "-s",
        SANDBOX_MODE,
        "-c",
        "approval_policy=\"never\"",
        "-c",
        "features.multi_agent=false",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub fn description() -> Value {
    json!({
        "approval_policy": APPROVAL_POLICY,
        "sandbox_mode": SANDBOX_MODE,
        "codex_permission_arguments": arguments(),
        "nested_agents": false,
        "concurrency_owner": "agent-research-lab",
        "host_boundary": "private task mount and PID namespace",
        "os_permission_errors": "Check target dependencies, mount permissions and namespace support; approvals do not grant host privileges."
    })
}

/// Model credentials may be explicitly selected, but host delivery authority,
/// loader hooks and namespace locations are never part of that interface.
pub fn validate_environment_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 100
            && name
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && name.as_bytes()[0].is_ascii_uppercase(),
        "invalid backend environment name"
    );
    ensure!(
        ![
            "GH_", "GITHUB_", "GIT_", "SSH_", "CODEX_", "LD_", "DYLD_", "BASH_", "PYTHON", "RUBY",
            "NODE_", "RELAY_", "LAB_", "XDG_", "CARGO_", "RUSTUP_"
        ]
        .iter()
        .any(|p| name.starts_with(p))
            && ![
                "HOME",
                "PATH",
                "SHELL",
                "ENV",
                "DISPLAY",
                "XAUTHORITY",
                "TMPDIR",
                "TMP",
                "TEMP"
            ]
            .contains(&name),
        "backend environment cannot grant host authority or alter isolation: {name}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_access_is_explicit_and_nested_agents_cannot_escape_the_global_limit() {
        let args = arguments();
        assert!(args.windows(2).any(|p| p == ["-s", "danger-full-access"]));
        assert!(
            args.windows(2)
                .any(|p| p == ["-c", "approval_policy=\"never\""])
        );
        assert!(
            args.windows(2)
                .any(|p| p == ["-c", "features.multi_agent=false"])
        );
    }
}
