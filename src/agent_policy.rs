//! One explicit Codex permission policy for every controller-owned agent.
//! The outer task namespace still keeps host credentials and receipts private.
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
