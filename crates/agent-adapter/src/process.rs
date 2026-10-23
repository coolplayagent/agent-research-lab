//! Bounded stdio sessions. No shell interpolation and no detached process trees.
use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::{Child, ChildStdin, Command, Stdio};
#[cfg(windows)]
type ChildStdin = std::fs::File;
#[cfg(windows)]
struct Child(windows_runtime::Process);
#[cfg(windows)]
impl Child {
    fn try_wait(&self) -> std::io::Result<Option<std::process::ExitStatus>> {
        use std::os::windows::process::ExitStatusExt;
        self.0
            .poll()
            .map(|code| code.map(std::process::ExitStatus::from_raw))
            .map_err(std::io::Error::other)
    }
    fn kill(&self) -> std::io::Result<()> {
        self.0.terminate();
        Ok(())
    }
    fn wait(&self) -> std::io::Result<()> {
        self.0.terminate();
        Ok(())
    }
}
pub struct Session {
    child: Child,
    input: Option<ChildStdin>,
    lines: Option<Receiver<Result<String>>>,
    readers: Vec<JoinHandle<()>>,
    deadline: Instant,
}
impl Session {
    pub fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
        env: &[(String, String)],
        seconds: u64,
        private_diagnostics: bool,
    ) -> Result<Self> {
        #[cfg(unix)]
        let (child, input, stdout, stderr) = {
            use std::os::unix::process::CommandExt;
            let mut child = Command::new(program)
                .args(args)
                .current_dir(cwd)
                .envs(env.iter().cloned())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()
                .context("launch agent program")?;
            let stdout = child.stdout.take().context("agent stdout unavailable")?;
            let stderr = child.stderr.take().context("agent stderr unavailable")?;
            let input = child.stdin.take();
            (child, input, stdout, stderr)
        };
        #[cfg(windows)]
        let (child, input, stdout, stderr) = {
            let mut environment: std::collections::BTreeMap<String, String> =
                std::env::vars().collect();
            environment.extend(env.iter().cloned());
            let command = std::iter::once(program.to_string_lossy().into_owned())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>();
            let (child, input, stdout, stderr) = windows_runtime::spawn_piped(
                &command,
                cwd,
                &environment.into_iter().collect::<Vec<_>>(),
            )?;
            (Child(child), Some(input), stdout, stderr)
        };
        let (tx, rx) = mpsc::channel();
        let err_tx = tx.clone();
        let readers = vec![
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut total = 0usize;
                loop {
                    let mut bytes = Vec::new();
                    let read = reader
                        .by_ref()
                        .take(256 * 1024 + 1)
                        .read_until(b'\n', &mut bytes);
                    match read {
                        Ok(0) => break,
                        Ok(_) => {
                            total += bytes.len();
                            if bytes.len() > 256 * 1024 || total > 4 * 1024 * 1024 {
                                let _ = tx.send(Err(anyhow::anyhow!("agent output exceeds bound")));
                                break;
                            }
                            let line =
                                String::from_utf8(bytes).context("agent output is not UTF-8");
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = tx.send(Err(error.into()));
                            break;
                        }
                    }
                }
            }),
            std::thread::spawn(move || {
                // Diagnostics are bounded and intentionally never returned as public API errors.
                let mut bytes = Vec::new();
                let result = stderr.take(64 * 1024 + 1).read_to_end(&mut bytes);
                if private_diagnostics {
                    let _ = std::io::stderr().write_all(&bytes[..bytes.len().min(64 * 1024)]);
                }
                if result.is_err() || bytes.len() > 64 * 1024 {
                    let _ = err_tx.send(Err(anyhow::anyhow!("agent diagnostics exceeded bound")));
                }
            }),
        ];
        Ok(Self {
            child,
            input,
            lines: Some(rx),
            readers,
            deadline: Instant::now() + Duration::from_secs(seconds),
        })
    }
    pub fn send(&mut self, data: &[u8]) -> Result<()> {
        ensure!(data.len() <= 256 * 1024, "agent input exceeds bound");
        let mut input = self.input.take().context("agent input is closed")?;
        let bytes = data.to_vec();
        let (tx, rx) = mpsc::channel();
        self.readers.push(std::thread::spawn(move || {
            let result = input.write_all(&bytes).and_then(|()| input.flush());
            let _ = tx.send((input, result));
        }));
        let (input, result) = rx
            .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
            .context("agent input timed out")?;
        self.input = Some(input);
        result?;
        Ok(())
    }
    pub fn close_input(&mut self) {
        self.input.take();
    }
    pub fn next(&mut self) -> Result<Option<String>> {
        loop {
            ensure!(Instant::now() < self.deadline, "agent execution timed out");
            match self
                .lines
                .as_ref()
                .context("agent output is closed")?
                .recv_timeout(Duration::from_millis(20))
            {
                Ok(result) => return result.map(Some),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    pub fn wait(&mut self) -> Result<()> {
        self.close_input();
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(
                    status.success(),
                    "agent exited unsuccessfully ({status}); inspect the private execution diagnostic"
                );
                return Ok(());
            }
            if Instant::now() >= self.deadline {
                bail!("agent execution timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.input.take();
        self.lines.take();
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
pub fn version(program: &Path, arguments: &[String]) -> Result<String> {
    let mut args = arguments.to_vec();
    args.push("--version".into());
    let cwd = std::env::temp_dir();
    let mut session = Session::spawn(program, &args, &cwd, &[], 5, false)?;
    session.close_input();
    let mut output = String::new();
    while let Some(line) = session.next()? {
        ensure!(
            output.len() + line.len() <= 4096,
            "agent version output exceeds bound"
        );
        output.push_str(&line);
    }
    session.wait()?;
    let version = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .context("agent returned no version")?
        .trim();
    ensure!(
        version.len() <= 256 && !version.chars().any(char::is_control),
        "invalid agent version output"
    );
    Ok(version.into())
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
