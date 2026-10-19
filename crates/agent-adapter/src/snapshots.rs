//! Copy only configuration and authentication into disposable private homes.
//! Session databases, conversation histories and experiment memories stay outside.
use anyhow::{Context, Result, ensure};
use contracts::CodingAgent;
use service_api::ResolvedAgent;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn runtime_paths(agent: &ResolvedAgent) -> Result<Vec<PathBuf>> {
    let mut paths = vec![agent.executable.clone()];
    for argument in &agent.arguments {
        let script = Path::new(argument);
        if !script.is_file() {
            continue;
        }
        let root = script
            .ancestors()
            .skip(1)
            .find(|p| p.join("package.json").is_file())
            .context("agent package root is missing")?;
        ensure!(
            root.components().count() > 2,
            "agent package root is too broad"
        );
        paths.push(root.to_path_buf());
    }
    Ok(paths)
}
pub fn environment_names(agent: CodingAgent) -> Vec<String> {
    let names: &[&str] = match agent {
        CodingAgent::Codex => &["OPENAI_API_KEY", "CODEX_API_KEY"],
        CodingAgent::ClaudeCode => &[
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ],
        CodingAgent::DeepseekHarness => &["DEEPSEEK_API_KEY", "DEEPSEEK_BASE_URL"],
        CodingAgent::Pi => &[
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "DEEPSEEK_API_KEY",
        ],
        CodingAgent::Custom => &[],
    };
    names
        .iter()
        .filter(|name| std::env::var_os(name).is_some())
        .map(|s| (*s).into())
        .collect()
}
fn home() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .context("user home is unavailable")
}
fn copy_file(source: &Path, target: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 1024 * 1024,
        "agent configuration must be a bounded regular file"
    );
    storage::private_dir(target.parent().context("configuration parent is missing")?)?;
    let bytes = fs::read(source)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "agent configuration exceeds bound"
    );
    fs::write(target, bytes)?;
    storage::private_file(target)?;
    Ok(())
}
/// Returns only environment paths and an optional model route, never credentials.
pub fn prepare_home(command: &[String], target: &Path) -> Result<Vec<(String, String)>> {
    storage::private_dir(target)?;
    let mut environment = vec![
        ("HOME".into(), target.to_string_lossy().into_owned()),
        ("USERPROFILE".into(), target.to_string_lossy().into_owned()),
    ];
    let Some(binary) = command.first() else {
        return Ok(environment);
    };
    if !Path::new(binary)
        .file_stem()
        .is_some_and(|name| name == "outcome-hub-agent")
    {
        return Ok(environment);
    }
    let name = command
        .windows(2)
        .find(|w| w[0] == "--agent")
        .context("bridge agent is missing")?;
    let agent = CodingAgent::parse(&name[1])?;
    let home = home()?;
    match agent {
        CodingAgent::Codex => {
            let root = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            for file in ["config.toml", "auth.json"] {
                copy_file(&root.join(file), &target.join(".codex").join(file))?;
            }
            environment.push((
                "CODEX_HOME".into(),
                target.join(".codex").to_string_lossy().into_owned(),
            ));
        }
        CodingAgent::ClaudeCode => {
            let root = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".claude"));
            for file in [".credentials.json", "settings.json"] {
                copy_file(&root.join(file), &target.join(".claude").join(file))?;
            }
            copy_file(&home.join(".claude.json"), &target.join(".claude.json"))?;
            environment.push((
                "CLAUDE_CONFIG_DIR".into(),
                target.join(".claude").to_string_lossy().into_owned(),
            ));
        }
        CodingAgent::Pi => {
            let root = std::env::var_os("PI_CODING_AGENT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".pi/agent"));
            for file in ["auth.json", "models.json", "settings.json"] {
                copy_file(&root.join(file), &target.join(".pi/agent").join(file))?;
            }
            environment.push((
                "PI_CODING_AGENT_DIR".into(),
                target.join(".pi/agent").to_string_lossy().into_owned(),
            ));
        }
        CodingAgent::DeepseekHarness => {
            let root = std::env::var_os("DSH_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".dsh"));
            for file in [
                "cordis.patch.yml",
                "profiles/sdk/cordis.patch.yml",
                "profiles/sdk/package.json",
                "auth.json",
            ] {
                copy_file(&root.join(file), &target.join(".dsh").join(file))?;
            }
            environment.push((
                "DSH_HOME".into(),
                target.join(".dsh").to_string_lossy().into_owned(),
            ));
        }
        CodingAgent::Custom => {}
    }
    environment.push((
        "OUTCOME_HUB_CONFIG_DIGEST".into(),
        snapshot_digest(agent, target)?,
    ));
    Ok(environment)
}
fn snapshot_digest(agent: CodingAgent, home: &Path) -> Result<String> {
    let files: &[&str] = match agent {
        CodingAgent::Codex => &[".codex/config.toml"],
        CodingAgent::ClaudeCode => &[".claude/settings.json", ".claude.json"],
        CodingAgent::Pi => &[".pi/agent/settings.json", ".pi/agent/models.json"],
        CodingAgent::DeepseekHarness => &[
            ".dsh/cordis.patch.yml",
            ".dsh/profiles/sdk/cordis.patch.yml",
            ".dsh/profiles/sdk/package.json",
        ],
        CodingAgent::Custom => &[],
    };
    let mut bytes = Vec::new();
    for relative in files {
        bytes.extend(relative.as_bytes());
        bytes.push(0);
        let path = home.join(relative);
        if path.is_file() {
            let content = fs::read(path)?;
            bytes.extend((content.len() as u64).to_le_bytes());
            bytes.extend(content);
        } else {
            bytes.extend(0u64.to_le_bytes());
        }
    }
    Ok(storage::digest(&bytes))
}
pub fn configuration_digest(agent: CodingAgent) -> Result<String> {
    let temporary = tempfile::tempdir()?;
    let command = vec![
        "outcome-hub-agent".into(),
        "--agent".into(),
        agent.to_string(),
    ];
    let environment = prepare_home(&command, temporary.path())?;
    environment
        .into_iter()
        .find(|(name, _)| name == "OUTCOME_HUB_CONFIG_DIGEST")
        .map(|(_, value)| value)
        .context("agent configuration digest is missing")
}

#[cfg(test)]
#[path = "snapshots_tests.rs"]
mod tests;
