//! Service-unit identity, running artifacts and observed HTTP health.
use crate::{host::command, model::Policy};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::{Duration, Instant},
};

fn main_pid(unit: &str) -> Result<u32> {
    let pid = String::from_utf8(systemctl(&[
        "show".into(),
        "--property=MainPID".into(),
        "--value".into(),
        unit.into(),
    ])?)?
    .trim()
    .parse::<u32>()?;
    ensure!(pid > 0, "service has no running main process");
    Ok(pid)
}

pub(super) fn running_artifact(unit: &str) -> Result<String> {
    let pid = main_pid(unit)?;
    delivery::file_sha256(Path::new(&format!("/proc/{pid}/exe")))
}

pub(super) fn config_loaded_after_write(policy: &Policy) -> Result<()> {
    let started = String::from_utf8(systemctl(&[
        "show".into(),
        "--property=ExecMainStartTimestampMonotonic".into(),
        "--value".into(),
        policy.main_unit.clone(),
    ])?)?
    .trim()
    .parse::<u64>()?;
    let uptime = fs::read_to_string("/proc/uptime")?
        .split_whitespace()
        .next()
        .context("kernel uptime is unavailable")?
        .parse::<f64>()?;
    let age = fs::metadata(&policy.service_config)?
        .modified()?
        .elapsed()?
        .as_micros();
    let written = ((uptime * 1_000_000.0) as u128).saturating_sub(age);
    ensure!(
        u128::from(started) >= written.saturating_add(20_000),
        "running service cannot be proven to have loaded the current config"
    );
    Ok(())
}

pub(super) fn systemctl(args: &[String]) -> Result<Vec<u8>> {
    command(
        &[vec!["systemctl".into(), "--user".into()], args.to_vec()].concat(),
        Path::new("/"),
        120,
    )
}

pub(super) fn unit(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 100
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
        "invalid activation unit name"
    );
    Ok(())
}

pub(super) fn service_health(policy: &Policy) -> Result<()> {
    let started = Instant::now();
    loop {
        let active = systemctl(&["is-active".into(), policy.main_unit.clone()]).is_ok();
        let http = http_health(&policy.health_url).is_ok();
        if active && http {
            return Ok(());
        }
        ensure!(
            started.elapsed() < Duration::from_secs(60),
            "activated service did not become healthy"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn http_health(url: &str) -> Result<()> {
    let location = url
        .strip_prefix("http://")
        .context("health URL must use HTTP")?;
    let (authority, path) = location.split_once('/').unwrap_or((location, ""));
    let address: SocketAddr = authority
        .parse()
        .context("health URL requires an explicit IP and port")?;
    ensure!(!path.contains(['\r', '\n']), "invalid health path");
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
    )?;
    let mut bytes = Vec::new();
    stream.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "health response exceeds bound");
    let status = std::str::from_utf8(
        bytes
            .split(|byte| *byte == b'\n')
            .next()
            .unwrap_or_default(),
    )?;
    let code = status
        .split_whitespace()
        .nth(1)
        .context("health response status missing")?
        .parse::<u16>()?;
    ensure!(
        (200..300).contains(&code),
        "health endpoint returned a non-success response"
    );
    Ok(())
}

pub(super) fn executable(unit_text: &str) -> Result<&str> {
    let mut entries = unit_text
        .lines()
        .filter_map(|line| line.strip_prefix("ExecStart="));
    let value = entries.next().context("service unit lacks ExecStart")?;
    ensure!(
        entries.next().is_none(),
        "activation requires one explicit ExecStart"
    );
    let binary = value
        .split_whitespace()
        .next()
        .context("empty service command")?;
    ensure!(
        binary.starts_with('/') && !binary.contains(['\'', '"']),
        "activation requires an absolute unquoted service binary"
    );
    Ok(binary)
}

pub(super) fn config_binding(unit_text: &str, expected: &Path) -> Result<()> {
    executable(unit_text)?;
    let line = unit_text
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))
        .unwrap();
    let arguments: Vec<_> = line.split_whitespace().collect();
    let path = arguments
        .windows(2)
        .find_map(|pair| (pair[0] == "--config").then_some(pair[1]))
        .or_else(|| {
            arguments
                .iter()
                .find_map(|value| value.strip_prefix("--config="))
        })
        .context("service unit does not select the prepared config")?;
    ensure!(
        fs::canonicalize(path)? == fs::canonicalize(expected)?,
        "service unit selects a different config"
    );
    Ok(())
}

pub(super) fn replace_executable(unit_text: &str, binary: &Path) -> Result<String> {
    let old = executable(unit_text)?;
    ensure!(
        !binary
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .any(u8::is_ascii_whitespace),
        "service artifact path contains whitespace"
    );
    Ok(unit_text.replacen(
        &format!("ExecStart={old}"),
        &format!("ExecStart={}", binary.display()),
        1,
    ))
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
