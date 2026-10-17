//! Independently deployable AI-IM. Applications attach routes to this host;
//! the core has no research controller, experiment configuration or CLI dependency.
mod agents;
mod design_review;
pub mod execution;
mod goals;
pub mod iteration;
mod remote_scenario;
mod scenarios;
mod services;
pub use scenarios::{
    Collaboration, ConversationDefaults, Scenario, ScenarioAdapter, ScenarioField,
    default_scenarios,
};
mod assets;
pub mod operator;
mod outcome_knowledge;
mod outcomes;
mod research;
mod rooms;
mod server;
pub mod transport;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
pub use server::{serve, serve_with_local_operator};
/// Bind one advertised interface address; remote management always requires credentials.
pub fn validate_listen(listen: SocketAddr, local_operator: bool) -> Result<()> {
    ensure!(
        !listen.ip().is_unspecified() && !listen.ip().is_multicast(),
        "select an explicit unicast interface address"
    );
    ensure!(
        !local_operator || listen.ip().is_loopback(),
        "local operator requires a loopback address"
    );
    Ok(())
}
use std::{
    fs::{self, File},
    io::Read,
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};
fn allowed_request(request: &str, addr: SocketAddr) -> Result<&str> {
    ensure!(request.ends_with("\r\n\r\n"), "incomplete headers");
    let mut lines = request.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    ensure!(
        first.len() == 3 && first[0] == "GET" && first[2] == "HTTP/1.1",
        "GET required"
    );
    let mut host = None;
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').context("invalid header")?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            ensure!(host.replace(value).is_none(), "duplicate host");
        }
        if name.eq_ignore_ascii_case("origin") {
            ensure!(
                value == format!("http://{addr}")
                    || (addr.ip().is_loopback()
                        && value == format!("http://localhost:{}", addr.port())),
                "foreign origin"
            );
        }
        if name.eq_ignore_ascii_case("sec-fetch-site") {
            ensure!(value == "same-origin" || value == "none", "foreign site");
        }
    }
    ensure!(
        host == Some(addr.to_string().as_str())
            || (addr.ip().is_loopback()
                && host == Some(format!("localhost:{}", addr.port()).as_str())),
        "foreign host"
    );
    Ok(first[1])
}

#[cfg(test)]
mod goal_tests;
#[cfg(test)]
mod iteration_tests;
#[cfg(test)]
mod outcome_tests;
#[cfg(test)]
mod tests;
