use anyhow::{Context, Result, ensure};
use service_api::SandboxOutput;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows_runtime::Process;
#[path = "snapshots.rs"]
mod snapshots;
#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;

pub async fn probe(state: &Path) -> Result<()> {
    let binary = std::env::current_exe()?;
    #[cfg(not(test))]
    let arguments = vec!["--sandbox-probe".into()];
    #[cfg(test)]
    let arguments = vec![
        "--exact".into(),
        "windows::tests::sandbox_health_probe".into(),
        "--nocapture".into(),
    ];
    let output = run(
        state,
        std::iter::once(binary.to_string_lossy().into_owned())
            .chain(arguments)
            .collect(),
        vec![],
        vec![binary],
        false,
        String::new(),
        5,
    )
    .await?;
    ensure!(
        output.stdout.contains("outcome-hub-sandbox-probe-v1"),
        "sandbox probe response is missing"
    );
    Ok(())
}
pub fn probe_child() -> Result<()> {
    windows_runtime::require_app_container()?;
    let contents = b"outcome-hub-sandbox-probe-v1";
    fs::write("runtime-probe.txt", contents)?;
    ensure!(
        fs::read("runtime-probe.txt")? == contents,
        "sandbox workspace probe failed"
    );
    fs::remove_file("runtime-probe.txt")?;
    println!("outcome-hub-sandbox-probe-v1");
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub async fn run(
    state: &Path,
    command: Vec<String>,
    env_names: Vec<String>,
    paths: Vec<PathBuf>,
    network: bool,
    input: String,
    seconds: u64,
) -> Result<SandboxOutput> {
    execute(
        state, command, env_names, paths, network, input, seconds, false,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub async fn verify(
    state: &Path,
    command: Vec<String>,
    env_names: Vec<String>,
    paths: Vec<PathBuf>,
    network: bool,
    input: String,
    seconds: u64,
) -> Result<SandboxOutput> {
    execute(
        state, command, env_names, paths, network, input, seconds, true,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn execute(
    state: &Path,
    command: Vec<String>,
    env_names: Vec<String>,
    paths: Vec<PathBuf>,
    network: bool,
    input: String,
    seconds: u64,
    verify: bool,
) -> Result<SandboxOutput> {
    let state = windows_runtime::user_path(state)?;
    let stop = Arc::new(AtomicBool::new(false));
    let _cancel = CancelOnDrop(stop.clone());
    let started = Instant::now();
    tokio::task::spawn_blocking(move || {
        let check = || {
            ensure!(!stop.load(Ordering::Acquire), "sandbox execution cancelled");
            ensure!(
                started.elapsed() < Duration::from_secs(seconds),
                "sandbox execution timed out"
            );
            Ok(())
        };
        execute_blocking(
            &state, command, env_names, paths, network, input, seconds, verify, started, &check,
        )
    })
    .await?
}
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
#[allow(clippy::too_many_arguments)]
fn execute_blocking(
    state: &Path,
    mut command: Vec<String>,
    env_names: Vec<String>,
    paths: Vec<PathBuf>,
    network: bool,
    mut input: String,
    seconds: u64,
    verify: bool,
    started: Instant,
    check: &impl Fn() -> Result<()>,
) -> Result<SandboxOutput> {
    check()?;
    service_api::Adapter {
        id: "sandbox-input".into(),
        agent: contracts::CodingAgent::Custom,
        protocol: contracts::AgentProtocol::JsonStdioV1,
        enabled: true,
        executor_id: "executor".into(),
        command: command.clone(),
        env_names: env_names.clone(),
        read_only_paths: paths.clone(),
        network,
        max_seconds: seconds,
    }
    .validate()?;
    ensure!(input.len() <= 256 * 1024, "sandbox input exceeds bound");
    let temp = tempfile::Builder::new().prefix("run-").tempdir_in(state)?;
    storage::private_dir(temp.path())?;
    let work = temp.path().join("workspace");
    storage::private_dir(&work)?;
    let home = temp.path().join("home");
    let mut environment = agent_adapter::prepare_home(&command, &home)?;
    for name in env_names {
        let value =
            std::env::var(&name).context("configured credential environment is unavailable")?;
        environment.push((name, value));
    }
    let profile = format!(
        "OutcomeHub.{}",
        &storage::digest(temp.path().as_os_str().to_string_lossy().as_bytes())[..32]
    );
    let profile = crate::profiles::Profile::create(state, &profile)?;
    let container = profile.container();
    let runtime = temp.path().join("runtime");
    storage::private_dir(&runtime)?;
    let mut remaining_bytes = 1024 * 1024 * 1024u64;
    let mut remaining_files = 50000usize;
    for (index, source) in paths.iter().enumerate() {
        let source = fs::canonicalize(source)?;
        let target = runtime.join(index.to_string()).join(
            source
                .file_name()
                .context("runtime path has no file name")?,
        );
        snapshots::copy(
            &source,
            &target,
            &mut remaining_bytes,
            &mut remaining_files,
            0,
            check,
        )?;
        for argument in &mut command {
            let path = fs::canonicalize(Path::new(argument))
                .unwrap_or_else(|_| PathBuf::from(argument.as_str()));
            if let Ok(relative) = path.strip_prefix(&source) {
                let staged = if relative.as_os_str().is_empty() {
                    target.clone()
                } else {
                    target.join(relative)
                };
                *argument = staged.to_string_lossy().into_owned();
            }
        }
    }
    container.grant(&runtime, false, check)?;
    container.grant(&work, true, check)?;
    container.grant(&home, true, check)?;
    let scratch = work.join("tmp");
    fs::create_dir(&scratch)?;
    for name in ["TEMP", "TMP"] {
        environment.push((name.into(), scratch.to_string_lossy().into_owned()));
    }
    for (name, relative) in [
        ("APPDATA", "AppData/Roaming"),
        ("LOCALAPPDATA", "AppData/Local"),
    ] {
        let directory = home.join(relative);
        fs::create_dir_all(&directory)?;
        environment.push((name.into(), directory.to_string_lossy().into_owned()));
    }
    let system = std::env::var("SystemRoot").context("Windows system directory is unavailable")?;
    environment.extend([
        ("SystemRoot".into(), system.clone()),
        ("WINDIR".into(), system.clone()),
        (
            "PATH".into(),
            format!(
                "{};{};{}",
                Path::new(&command[0])
                    .parent()
                    .context("program directory is missing")?
                    .display(),
                Path::new(&system).join("System32").display(),
                Path::new(&system)
                    .join("System32/WindowsPowerShell/v1.0")
                    .display()
            ),
        ),
        ("OUTCOME_HUB_ISOLATED".into(), "1".into()),
    ]);
    let challenge = if verify {
        Some(crate::challenge::Challenge::prepare(&work, &mut input)?)
    } else {
        None
    };
    let input_sha256 = storage::digest(input.as_bytes());
    let input_path = temp.path().join("input.json");
    fs::write(&input_path, input)?;
    let output_path = temp.path().join("output.json");
    let error_path = temp.path().join("diagnostic.log");
    check()?;
    let child = Process::spawn(
        Some(container),
        &command,
        &work,
        &environment,
        network,
        &fs::File::open(input_path)?,
        &fs::File::create(&output_path)?,
        &fs::File::create(&error_path)?,
    )?;
    loop {
        check()?;
        ensure!(
            fs::metadata(&output_path)?.len() <= 256 * 1024,
            "sandbox protocol output exceeds bound"
        );
        ensure!(
            fs::metadata(&error_path)?.len() <= 4 * 1024 * 1024,
            "sandbox diagnostic output exceeds bound"
        );
        let mut byte_budget = 256 * 1024 * 1024;
        let mut file_budget = 10000;
        snapshots::budget(&work, &mut byte_budget, &mut file_budget, 0, check)?;
        snapshots::budget(&home, &mut byte_budget, &mut file_budget, 0, check)?;
        if let Some(code) = child.poll()? {
            if code != 0 {
                drop(child);
                let diagnostic = crate::diagnostics::capture(
                    state,
                    &error_path,
                    &temp.path().to_string_lossy(),
                    code,
                )
                .unwrap_or_else(|_| "unavailable".into());
                anyhow::bail!(
                    "sandbox command failed (exit code {code}; diagnostic {diagnostic}); verify the agent login and model configuration"
                );
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(child);
    if let Some(challenge) = &challenge {
        challenge.verify(&work)?;
    }
    let stdout = fs::read_to_string(output_path)?;
    ensure!(
        stdout.len() <= 256 * 1024,
        "sandbox protocol output exceeds bound"
    );
    Ok(SandboxOutput {
        stdout,
        elapsed_ms: started.elapsed().as_millis() as u64,
        sandbox_id: "windows-appcontainer-job-v1".into(),
        challenge_verified: challenge.map(|_| true),
        input_sha256: Some(input_sha256),
    })
}
