use anyhow::{Context, Result, bail, ensure};
use contracts::CodingAgent;
use service_api::ResolvedAgent;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn basename(agent: CodingAgent) -> Result<&'static str> {
    Ok(match agent {
        CodingAgent::Codex => "codex",
        CodingAgent::ClaudeCode => "claude",
        CodingAgent::DeepseekHarness => "dsh",
        CodingAgent::Pi => "pi",
        CodingAgent::Custom => bail!("custom adapter requires an explicit protocol command"),
    })
}
pub(super) fn companion(program: &Path) -> Option<PathBuf> {
    if !program.file_stem().is_some_and(|name| name == "codex") {
        return None;
    }
    let path = program.with_file_name(if program.extension().is_some_and(|ext| ext == "exe") {
        "codex-code-mode-host.exe"
    } else {
        "codex-code-mode-host"
    });
    path.is_file().then_some(path)
}
pub fn executable_digest(program: &Path, arguments: &[String]) -> Result<String> {
    let mut hash = Sha256::new();
    for path in std::iter::once(program.to_path_buf())
        .chain(arguments.iter().map(PathBuf::from).filter(|p| p.is_file()))
        .chain(companion(program))
    {
        let mut file = fs::File::open(&path)?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() <= 512 * 1024 * 1024,
            "agent executable exceeds bound"
        );
        hash.update(file.metadata()?.len().to_le_bytes());
        let mut bytes = [0u8; 65536];
        loop {
            let size = file.read(&mut bytes)?;
            if size == 0 {
                break;
            }
            hash.update(&bytes[..size]);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn resolve(agent: CodingAgent, hint: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = hint {
        ensure!(
            path.is_absolute() && path.is_file(),
            "agent program must be an existing absolute file"
        );
        return Ok(fs::canonicalize(path)?);
    }
    let base = basename(agent)?;
    let names = if cfg!(windows) {
        vec![format!("{base}.exe"), format!("{base}.cmd")]
    } else {
        vec![base.into()]
    };
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let mut directories: Vec<PathBuf> = std::env::split_paths(&paths)
        .filter(|p| p.is_absolute())
        .collect();
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        directories.push(PathBuf::from(profile).join(".local/bin"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        directories.push(PathBuf::from(appdata).join("npm"));
    }
    for directory in directories.into_iter().take(128) {
        for name in &names {
            let path = directory.join(name);
            if path.is_file() {
                return Ok(fs::canonicalize(path)?);
            }
        }
    }
    bail!("agent program not found; install it or select its absolute executable path")
}
fn npm_entry(agent: CodingAgent, shim: &Path) -> Result<(PathBuf, Vec<String>)> {
    let packages: &[&str] = match agent {
        CodingAgent::Codex => &["@openai/codex"],
        CodingAgent::ClaudeCode => &["@anthropic-ai/claude-code"],
        CodingAgent::DeepseekHarness => &["@deepseek-ai/dsh"],
        CodingAgent::Pi => &[
            "@earendil-works/pi-coding-agent",
            "@mariozechner/pi-coding-agent",
        ],
        CodingAgent::Custom => bail!("custom command shim is not supported"),
    };
    let parent = shim.parent().context("agent shim requires a parent")?;
    for package in packages {
        let root = parent.join("node_modules").join(package);
        let manifest = root.join("package.json");
        if !manifest.is_file() {
            continue;
        }
        let metadata = fs::metadata(&manifest)?;
        ensure!(
            metadata.len() < 256 * 1024,
            "agent package manifest exceeds bound"
        );
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
        let bin = value["bin"]
            .as_str()
            .or_else(|| value["bin"][basename(agent).ok()?].as_str())
            .context("agent package has no supported CLI entry")?;
        let script = fs::canonicalize(root.join(bin))?;
        ensure!(
            script.starts_with(fs::canonicalize(&root)?),
            "agent package entry escapes installation"
        );
        #[cfg(windows)]
        let script = windows_runtime::user_path(&script)?;
        let node = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join("node.exe"))
            .find(|p| p.is_file())
            .context("Node.js runtime is required for this agent installation")?;
        return Ok((
            fs::canonicalize(node)?,
            vec![
                // The isolated copy contains no symlinks. Avoid Node's ancestor
                // realpath probes, which require access outside that copy.
                "--preserve-symlinks-main".into(),
                "--preserve-symlinks".into(),
                script
                    .to_str()
                    .context("agent script path must be Unicode")?
                    .into(),
            ],
        ));
    }
    bail!(
        "unrecognized command shim; select a native executable or install the official CLI package"
    )
}
pub fn discover(agent: CodingAgent, hint: Option<&Path>) -> Result<ResolvedAgent> {
    let path = resolve(agent, hint).context(DiscoveryFailure(
        contracts::BindingErrorCode::ProgramMissing,
    ))?;
    let (executable, arguments) = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd"))
    {
        npm_entry(agent, &path).context(DiscoveryFailure(
            contracts::BindingErrorCode::ProgramInvalid,
        ))?
    } else {
        (path, vec![])
    };
    rediscover(agent, executable, arguments)
}
/// Recheck a previously resolved native or Node launcher without losing its arguments.
pub fn rediscover(
    agent: CodingAgent,
    executable: PathBuf,
    arguments: Vec<String>,
) -> Result<ResolvedAgent> {
    let version = crate::process::version(&executable, &arguments)
        .context(DiscoveryFailure(contracts::BindingErrorCode::LaunchFailed))?;
    let digest = executable_digest(&executable, &arguments)?;
    Ok(ResolvedAgent {
        agent,
        executable,
        arguments,
        version,
        digest,
    })
}
#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;

#[derive(Debug)]
pub struct DiscoveryFailure(pub contracts::BindingErrorCode);
impl std::fmt::Display for DiscoveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for DiscoveryFailure {}
pub fn discovery_failure_code(error: &anyhow::Error) -> Option<contracts::BindingErrorCode> {
    error
        .downcast_ref::<DiscoveryFailure>()
        .map(|failure| failure.0)
}
