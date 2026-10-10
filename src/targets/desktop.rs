//! Disposable real X11 desktop targets. No inherited desktop or session bus is used.
use crate::storage;
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const TARGET: &str = include_str!("../../assets/desktop/target.py");
const BOOTSTRAP: &str = include_str!("../../assets/desktop/bootstrap.py");
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Explicit and resumable preparation. Only downloads/extracts authenticated APT
/// packages beneath the requested private prefix; never runs system installation.
pub fn prepare(dependencies: &Path, timeout: Duration) -> Result<Value> {
    ensure!(
        timeout <= Duration::from_secs(60) && !timeout.is_zero(),
        "desktop preparation must be bounded to 1..60 seconds"
    );
    let output = crate::process::capture(
        "/usr/bin/python3",
        &[
            "-c".into(),
            BOOTSTRAP.into(),
            dependencies.display().to_string(),
            "--timeout".into(),
            timeout.as_secs().max(1).to_string(),
        ],
        Path::new("/"),
        timeout,
    )?;
    ensure!(
        output.status.success(),
        "desktop preparation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

struct OwnedChild {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
    stopped: bool,
}
impl OwnedChild {
    fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
        name: &str,
    ) -> Result<Self> {
        let stdout = cwd.join(format!("{name}.stdout"));
        let stderr = cwd.join(format!("{name}.stderr"));
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(env.iter().cloned())
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&stdout)?,
            ))
            .stderr(Stdio::from(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&stderr)?,
            ));
        Ok(Self {
            child: command
                .spawn()
                .with_context(|| format!("start {}", program.display()))?,
            stdout,
            stderr,
            stopped: false,
        })
    }
    fn alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            self.stop()?;
            bail!(
                "desktop process exited {status}: {}",
                bounded_text(&self.stderr)?
            );
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        // Clean up this process group exactly once, immediately after observing
        // its leader's exit or when stopping a live owned child. Repeated Drop
        // calls must never signal a PID that could have been reused later.
        self.stopped = true;
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        self.child.wait()?;
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub struct DesktopSession {
    output: PathBuf,
    dependencies: PathBuf,
    environment: Vec<(String, String)>,
    children: Vec<OwnedChild>,
    title: String,
    nonce: String,
    app_pid: u32,
    inspected: bool,
}

impl DesktopSession {
    /// `dependencies` must be state/tools/desktop and output a new directory
    /// beneath the same state's runs tree. Safe both inside an agent namespace
    /// and on the host: every command receives only the dedicated Xvfb authority.
    pub fn start(output: &Path, dependencies: &Path) -> Result<Self> {
        let dependencies = fs::canonicalize(dependencies)
            .context("desktop dependencies missing; run targets prepare")?;
        ensure!(
            dependencies
                .file_name()
                .is_some_and(|name| name == "desktop")
                && dependencies
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "tools"),
            "desktop dependencies must be under state/tools/desktop"
        );
        let state = dependencies
            .parent()
            .and_then(Path::parent)
            .context("missing desktop state root")?;
        let parent = fs::canonicalize(output.parent().context("missing output parent")?)?;
        ensure!(
            parent.starts_with(state.join("runs")),
            "desktop output must be beneath state/runs"
        );
        let name = output.file_name().context("missing output name")?;
        let output = parent.join(name);
        fs::create_dir(&output).context("desktop output must be a new private directory")?;
        fs::set_permissions(&output, fs::Permissions::from_mode(0o700))?;
        verify_dependencies(&dependencies)?;
        for path in ["home", "run", "config", "cache", "data"] {
            let directory = output.join(path);
            fs::create_dir(&directory)?;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let mut random = [0_u8; 20];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let nonce = storage::digest(&random)[..16].to_owned();
        let cookie: String = random[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let first_display = 2_000 + u16::from_be_bytes([random[16], random[17]]) as u32 % 50_000;
        let title = format!("Agent Research Lab Target {nonce}");
        let library = dependencies.join("usr/lib/x86_64-linux-gnu");
        let environment = vec![
            (
                "PATH".into(),
                format!("{}/usr/bin:/usr/bin:/bin", dependencies.display()),
            ),
            ("LD_LIBRARY_PATH".into(), library.display().to_string()),
            (
                "IMLIB2_LOADER_PATH".into(),
                library.join("imlib2/loaders").display().to_string(),
            ),
            ("HOME".into(), output.join("home").display().to_string()),
            (
                "XAUTHORITY".into(),
                output.join("authority").display().to_string(),
            ),
            (
                "XDG_RUNTIME_DIR".into(),
                output.join("run").display().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                output.join("config").display().to_string(),
            ),
            (
                "XDG_CACHE_HOME".into(),
                output.join("cache").display().to_string(),
            ),
            (
                "XDG_DATA_HOME".into(),
                output.join("data").display().to_string(),
            ),
            (
                "XDG_DATA_DIRS".into(),
                format!("{}/usr/share:/usr/share", dependencies.display()),
            ),
            ("GDK_BACKEND".into(), "x11".into()),
            ("QT_QPA_PLATFORM".into(), "xcb".into()),
            ("GSETTINGS_BACKEND".into(), "memory".into()),
            ("NO_AT_BRIDGE".into(), "1".into()),
            ("LIBGL_ALWAYS_SOFTWARE".into(), "1".into()),
            ("LANG".into(), "C.UTF-8".into()),
        ];
        let mut session = Self {
            output,
            dependencies,
            environment,
            children: vec![],
            title,
            nonce,
            app_pid: 0,
            inspected: false,
        };
        // A private cookie and fresh random display avoid touching any existing
        // socket. Collision retries never delete another server's socket/lock.
        for offset in 0..8 {
            let display = format!(":{}", first_display + offset);
            let number = first_display + offset;
            if Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
                || Path::new(&format!("/tmp/.X{number}-lock")).exists()
            {
                continue;
            }
            session.environment.retain(|(name, _)| name != "DISPLAY");
            session
                .environment
                .push(("DISPLAY".into(), display.clone()));
            session.command(
                Path::new("/usr/bin/xauth"),
                &[
                    "-f".into(),
                    session.output.join("authority").display().to_string(),
                    "add".into(),
                    display.clone(),
                    "MIT-MAGIC-COOKIE-1".into(),
                    cookie.clone(),
                ],
                Duration::from_secs(3),
            )?;
            let mut server = OwnedChild::spawn(
                &session.dependencies.join("usr/bin/Xvfb"),
                &[
                    display,
                    "-screen".into(),
                    "0".into(),
                    "1024x768x24".into(),
                    "-nolisten".into(),
                    "tcp".into(),
                    "-auth".into(),
                    session.output.join("authority").display().to_string(),
                    "-noreset".into(),
                    // This fixture uses software-rendered GTK. Disabling GLX
                    // avoids loading host GPU vendor drivers inside /dev isolation.
                    "-extension".into(),
                    "GLX".into(),
                ],
                &session.output,
                &session.environment,
                &format!("xvfb-{offset}"),
            )?;
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut ready = false;
            while Instant::now() < deadline {
                if server.alive().is_err() {
                    break;
                }
                if session
                    .command(Path::new("/usr/bin/xdpyinfo"), &[], Duration::from_secs(1))
                    .is_ok()
                {
                    ready = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if ready {
                session.children.push(server);
                break;
            }
        }
        ensure!(
            !session.children.is_empty(),
            "could not start private Xvfb; inspect owned xvfb logs"
        );
        let config = session.output.join("openbox.xml");
        fs::write(
            &config,
            r#"<openbox_config xmlns="http://openbox.org/3.4/rc"><focus><focusNew>yes</focusNew></focus><theme><name>Clearlooks</name></theme><desktops><number>1</number></desktops></openbox_config>"#,
        )?;
        session.children.push(OwnedChild::spawn(
            &session.dependencies.join("usr/bin/openbox"),
            &[
                "--config-file".into(),
                config.display().to_string(),
                "--sm-disable".into(),
            ],
            &session.output,
            &session.environment,
            "openbox",
        )?);
        let app = session.output.join("target.py");
        fs::write(&app, TARGET)?;
        let child = OwnedChild::spawn(
            Path::new("/usr/bin/python3"),
            &[
                app.display().to_string(),
                session.output.display().to_string(),
                session.nonce.clone(),
            ],
            &session.output,
            &session.environment,
            "target",
        )?;
        session.app_pid = child.child.id();
        session.children.push(child);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            session.check_children()?;
            if session.observation().is_ok() {
                storage::write(
                    &session.output.join("session.json"),
                    &json!({"schema_version":1,"kind":"real-xvfb","display":session.display(),"title":session.title,"app_pid":session.app_pid,"nonce":session.nonce,"dependencies_manifest_sha256":storage::digest(&fs::read(session.dependencies.join("manifest.json"))?)}),
                )?;
                return Ok(session);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        bail!("private GTK target did not become ready")
    }

    pub fn output_dir(&self) -> &Path {
        &self.output
    }
    pub fn window_title(&self) -> &str {
        &self.title
    }
    pub fn environment(&self) -> Vec<(String, String)> {
        self.environment.clone()
    }
    fn display(&self) -> &str {
        self.environment
            .iter()
            .find(|(key, _)| key == "DISPLAY")
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }

    /// Capture/validate the actual owned window before permitting input.
    pub fn inspect(&mut self, cli: &Path) -> Result<Value> {
        self.check_children()?;
        let observed = self.observation()?;
        let windows = self.cli(cli, "safe", &["list-windows".into()])?;
        require_owned_window(
            &windows,
            &self.title,
            observed["window_id"]
                .as_u64()
                .context("missing window id")?,
        )?;
        let screenshot = self.cli(
            cli,
            "safe",
            &[
                "screenshot".into(),
                "--out".into(),
                self.output.join("before.png").display().to_string(),
            ],
        )?;
        validate_png(&self.output.join("before.png"))?;
        self.inspected = true;
        let value = json!({"kind":"real-desktop-observation","window":observed,"windows":windows,"screenshot":screenshot,"screenshot_path":self.output.join("before.png"),"cli_sha256":storage::digest(&fs::read(cli)?)});
        storage::write(&self.output.join("inspection.json"), &value)?;
        Ok(value)
    }

    /// A real input round trip, never a fake runtime or a fabricated app result.
    pub fn verify(&mut self, cli: &Path, token: &str) -> Result<Value> {
        ensure!(self.inspected, "inspect the owned desktop before input");
        ensure!(
            !token.is_empty()
                && token.len() <= 128
                && token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)),
            "verification token must be 1..128 ASCII letters/digits/-/_"
        );
        self.check_children()?;
        let before = self.observation()?;
        let windows = self.cli(cli, "safe", &["list-windows".into()])?;
        require_owned_window(
            &windows,
            &self.title,
            before["window_id"].as_u64().context("missing window id")?,
        )?;
        self.cli(
            cli,
            "guarded",
            &["focus-window".into(), "--title".into(), self.title.clone()],
        )?;
        self.cli(
            cli,
            "guarded",
            &["hotkey".into(), "--keys".into(), "Ctrl+A".into()],
        )?;
        let input = self.cli(
            cli,
            "guarded",
            &["type-text".into(), "--text".into(), token.into()],
        )?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let observed = loop {
            self.check_children()?;
            let observed = self.observation()?;
            if observed["text"] == token
                && observed["key_events"].as_u64().unwrap_or(0)
                    > before["key_events"].as_u64().unwrap_or(0)
            {
                break observed;
            }
            ensure!(
                Instant::now() < deadline,
                "CLI input did not reach the owned GTK entry"
            );
            std::thread::sleep(Duration::from_millis(30));
        };
        let screenshot = self.cli(
            cli,
            "safe",
            &[
                "screenshot".into(),
                "--out".into(),
                self.output.join("after.png").display().to_string(),
            ],
        )?;
        validate_png(&self.output.join("after.png"))?;
        let value = json!({"verified":true,"kind":"real-xvfb-input-roundtrip","input":input,"observed":observed,"screenshot":screenshot,"screenshot_path":self.output.join("after.png"),"display":self.display(),"claim":"actual CLI input reached owned GTK application; this is not research quality approval"});
        storage::write(&self.output.join("verification.json"), &value)?;
        Ok(value)
    }

    pub fn stop(&mut self) -> Result<()> {
        let mut errors = vec![];
        for child in self.children.iter_mut().rev() {
            if let Err(error) = child.stop() {
                errors.push(error.to_string());
            }
        }
        self.children.clear();
        ensure!(
            errors.is_empty(),
            "desktop cleanup failed: {}",
            errors.join("; ")
        );
        Ok(())
    }
    fn check_children(&mut self) -> Result<()> {
        for child in &mut self.children {
            child.alive()?;
        }
        Ok(())
    }
    fn observation(&self) -> Result<Value> {
        let observed: Value = storage::read(&self.output.join("observed.json"))?;
        ensure!(
            observed["nonce"] == self.nonce
                && observed["title"] == self.title
                && observed["pid"] == self.app_pid,
            "GTK observation does not belong to this session"
        );
        Ok(observed)
    }
    fn cli(&self, program: &Path, risk: &str, arguments: &[String]) -> Result<Value> {
        let mut args = vec!["--allow-risk".into(), risk.into()];
        args.extend_from_slice(arguments);
        let text = self.command(program, &args, Duration::from_secs(15))?;
        let value: Value = serde_json::from_str(&text)?;
        ensure!(
            value["ok"] == true && value["data"]["runtime"] == "linux",
            "computer-use operation failed or used fake runtime: {value}"
        );
        Ok(value)
    }
    fn command(&self, program: &Path, args: &[String], timeout: Duration) -> Result<String> {
        let name = format!("command-{}", SEQUENCE.fetch_add(1, Ordering::Relaxed));
        let mut child = OwnedChild::spawn(program, args, &self.output, &self.environment, &name)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = child.child.try_wait()? {
                child.stop()?;
                let stdout = bounded_text(&child.stdout)?;
                ensure!(
                    status.success(),
                    "desktop command {} failed: {} {}",
                    program.display(),
                    stdout,
                    bounded_text(&child.stderr)?
                );
                return Ok(stdout);
            }
            ensure!(
                Instant::now() < deadline,
                "desktop command timed out: {}",
                program.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for DesktopSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn bounded_text(path: &Path) -> Result<String> {
    ensure!(
        fs::metadata(path)?.len() <= 1024 * 1024,
        "desktop output exceeded 1 MiB"
    );
    Ok(fs::read_to_string(path)?)
}
fn verify_dependencies(root: &Path) -> Result<()> {
    let manifest: Value = storage::read(&root.join("manifest.json"))?;
    ensure!(
        manifest["source"] == "authenticated-apt-index",
        "desktop dependencies need authenticated APT provenance"
    );
    let files = manifest["files"]
        .as_object()
        .context("desktop manifest lacks file hashes")?;
    ensure!(!files.is_empty(), "empty desktop dependency manifest");
    for (path, expected) in files {
        ensure!(
            Path::new(path)
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
            "invalid dependency path"
        );
        let actual = root.join(path);
        ensure!(
            !fs::symlink_metadata(&actual)?.file_type().is_symlink()
                && storage::digest(&fs::read(actual)?)
                    == expected.as_str().context("invalid dependency hash")?,
            "desktop dependency bytes changed: {path}"
        );
    }
    for binary in ["Xvfb", "xdotool", "scrot", "openbox"] {
        ensure!(
            files.contains_key(&format!("usr/bin/{binary}")),
            "desktop executable missing from manifest: {binary}"
        );
    }
    let symlinks = manifest["symlinks"]
        .as_object()
        .context("desktop manifest lacks symlink bindings")?;
    for (path, expected) in symlinks {
        ensure!(
            Path::new(path)
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
            "invalid dependency symlink path"
        );
        ensure!(
            fs::read_link(root.join(path))?
                == Path::new(expected.as_str().context("invalid symlink binding")?),
            "desktop dependency symlink changed: {path}"
        );
    }
    Ok(())
}
fn require_owned_window(reply: &Value, title: &str, id: u64) -> Result<()> {
    let windows = reply["observation"]["windows"]
        .as_array()
        .context("computer-use did not return observed windows")?;
    let matching: Vec<_> = windows
        .iter()
        .filter(|window| window["title"] == title)
        .collect();
    ensure!(
        matching.len() == 1,
        "owned target window missing or ambiguous"
    );
    let actual = matching[0]["id"]
        .as_str()
        .context("window id must be a string")?;
    let actual = if let Some(hex) = actual.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        actual.parse()?
    };
    ensure!(actual == id, "window identity changed");
    Ok(())
}
fn validate_png(path: &Path) -> Result<()> {
    let mut header = [0_u8; 24];
    File::open(path)?.read_exact(&mut header)?;
    ensure!(
        &header[..8] == b"\x89PNG\r\n\x1a\n"
            && u32::from_be_bytes(header[16..20].try_into()?) == 1024
            && u32::from_be_bytes(header[20..24].try_into()?) == 768,
        "screenshot is not the dedicated 1024x768 desktop"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_must_identify_the_exact_owned_window() {
        let window = json!({"observation":{"windows":[{"id":"0x200003","title":"owned"}]}});
        require_owned_window(&window, "owned", 0x200003).unwrap();
        assert!(require_owned_window(&window, "owned", 0x200004).is_err());
        assert!(require_owned_window(&window, "personal", 0x200003).is_err());
        let duplicate = json!({"observation":{"windows":[{"id":"2","title":"owned"},{"id":"3","title":"owned"}]}});
        assert!(require_owned_window(&duplicate, "owned", 2).is_err());
    }
    #[test]
    fn cleanup_cancels_only_its_own_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let mut unrelated = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let owned = OwnedChild::spawn(
            Path::new("/bin/sleep"),
            &["30".into()],
            directory.path(),
            &[],
            "owned",
        )
        .unwrap();
        let id = owned.child.id();
        drop(owned);
        assert!(!Path::new(&format!("/proc/{id}")).exists());
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
    }
    #[test]
    fn rejects_manifest_tampering_and_unsafe_output_layout() {
        let directory = tempfile::tempdir().unwrap();
        let dependencies = directory.path().join("tools/desktop");
        fs::create_dir_all(&dependencies).unwrap();
        assert!(DesktopSession::start(&directory.path().join("outside"), &dependencies).is_err());
        storage::write(
            &dependencies.join("manifest.json"),
            &json!({"source":"authenticated-apt-index","files":{"../outside":"digest"}}),
        )
        .unwrap();
        assert!(verify_dependencies(&dependencies).is_err());
        fs::create_dir_all(dependencies.join("usr/bin")).unwrap();
        fs::create_dir_all(dependencies.join("usr/lib")).unwrap();
        let mut files = serde_json::Map::new();
        for binary in ["Xvfb", "xdotool", "scrot", "openbox"] {
            let path = format!("usr/bin/{binary}");
            fs::write(dependencies.join(&path), b"fixture executable").unwrap();
            files.insert(path, json!(storage::digest(b"fixture executable")));
        }
        let link = dependencies.join("usr/lib/example.so");
        std::os::unix::fs::symlink("../bin/Xvfb", &link).unwrap();
        storage::write(
            &dependencies.join("manifest.json"),
            &json!({"source":"authenticated-apt-index",
            "files":files,"symlinks":{"usr/lib/example.so":"../bin/Xvfb"}}),
        )
        .unwrap();
        verify_dependencies(&dependencies).unwrap();
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../bin/xdotool", &link).unwrap();
        assert!(verify_dependencies(&dependencies).is_err());
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink("../bin/Xvfb", &link).unwrap();
        fs::write(dependencies.join("usr/bin/Xvfb"), b"changed bytes").unwrap();
        assert!(verify_dependencies(&dependencies).is_err());
    }
    #[test]
    #[ignore = "set LAB_DESKTOP_DEPENDENCIES and LAB_COMPUTER_USE_BIN for an actual dedicated desktop"]
    fn actual_desktop_observation_input_and_cleanup() {
        let dependencies = PathBuf::from(std::env::var("LAB_DESKTOP_DEPENDENCIES").unwrap());
        let cli = PathBuf::from(std::env::var("LAB_COMPUTER_USE_BIN").unwrap());
        let runs = dependencies
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("runs");
        fs::create_dir_all(&runs).unwrap();
        let output = runs.join(format!(
            "desktop-integration-{}-{}",
            std::process::id(),
            crate::workflow::now_ms().unwrap()
        ));
        let mut session = DesktopSession::start(&output, &dependencies).unwrap();
        session.inspect(&cli).unwrap();
        if let Ok(wait_file) = std::env::var("LAB_DESKTOP_INSPECTION_GATE") {
            println!(
                "inspect screenshot: {}",
                output.join("before.png").display()
            );
            storage::write(
                Path::new(&wait_file).with_extension("ready.json").as_path(),
                &json!({"output":output}),
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(120);
            while !Path::new(&wait_file).exists() {
                assert!(Instant::now() < deadline, "inspection gate timed out");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        assert_eq!(
            session.verify(&cli, "lab-owned-42").unwrap()["verified"],
            true
        );
        let ids: Vec<_> = session
            .children
            .iter()
            .map(|child| child.child.id())
            .collect();
        session.stop().unwrap();
        assert!(
            ids.iter()
                .all(|id| !Path::new(&format!("/proc/{id}")).exists())
        );
        println!("real desktop evidence: {}", output.display());
    }
}
