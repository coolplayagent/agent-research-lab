use super::transport::{Web, json_response, response};
use super::*;
use axum::{http::StatusCode, response::Response};
pub(super) fn get(path: &str, web: &Web) -> Response {
    let asset = match path {
        "/" => Some((
            "text/html; charset=utf-8",
            include_bytes!("index.html").as_slice(),
        )),
        "/operator.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("operator.js").as_slice(),
        )),
        "/app.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("app.js").as_slice(),
        )),
        "/people.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("people.js").as_slice(),
        )),
        "/lineage.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("lineage.js").as_slice(),
        )),
        "/crystal.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("crystal.js").as_slice(),
        )),
        "/style.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("style.css").as_slice(),
        )),
        _ => None,
    };
    if let Some((mime, body)) = asset {
        return response(StatusCode::OK, mime, body);
    }
    if path == "/api/snapshot" {
        return json_response(Ok(web.snapshot.read().unwrap().clone()));
    }
    let result = (|| -> Result<Value> {
        let c = web
            .config
            .as_deref()
            .context("identity service unavailable")?;
        if path == "/api/evolution/lineage" || path.starts_with("/api/evolution/lineage?after=") {
            let file = c.state_dir.join("evolution/lineage.json");
            if !file.exists() {
                return Ok(
                    json!({"available":false,"notice":"尚未导入开发集谱系证据。策略归档与正式评估保持独立。"}),
                );
            }
            let receipt: evolution_cli::LineageReceipt = storage::read(&file)?;
            receipt.validate()?;
            let mut report = receipt.report;
            let after = path
                .strip_prefix("/api/evolution/lineage?after=")
                .unwrap_or("");
            let total = report.nodes.len();
            report.nodes.retain(|node| node.version.as_str() > after);
            let next = (report.nodes.len() > 100).then(|| report.nodes[99].version.clone());
            report.nodes.truncate(100);
            return Ok(json!({"available":true,"report":report,"total":total,"next_after":next}));
        }
        if path == "/api/people" || path.starts_with("/api/people?after=") {
            return personas::list(c, path.strip_prefix("/api/people?after=").unwrap_or(""));
        }
        if let Some(rest) = path.strip_prefix("/api/people/") {
            let (id, after) = rest.split_once("?after=").unwrap_or((rest, ""));
            return personas::history(c, id, after, &web.snapshot);
        }
        if let Some(run) = path.strip_prefix("/api/profile/") {
            let job = web.snapshot.read().unwrap()["jobs"]
                .as_array()
                .and_then(|jobs| jobs.iter().find(|j| j["run_id"] == run))
                .cloned()
                .or_else(|| {
                    personas::historical_job(&c.state_dir, run)
                        .ok()
                        .map(|j| project(&j, &json!({"state":"historical"})))
                })
                .context("unknown profile")?;
            return Ok(profiles::read(&c.state_dir, &c.workspace, &job));
        }
        if let Some(rest) = path.strip_prefix("/api/session/") {
            let (run, query) = rest.split_once('?').unwrap_or((rest, ""));
            let before = if query.is_empty() {
                None
            } else {
                Some(
                    query
                        .strip_prefix("before=")
                        .context("invalid cursor")?
                        .parse::<u64>()?,
                )
            };
            let known = web.snapshot.read().unwrap()["jobs"]
                .as_array()
                .is_some_and(|jobs| jobs.iter().any(|j| j["run_id"] == run));
            ensure!(
                known || personas::historical_job(&c.state_dir, run).is_ok(),
                "unknown session"
            );
            return sessions::page(&c.state_dir, run, before);
        }
        anyhow::bail!("not found")
    })();
    match result {
        Ok(value) => json_response(Ok(value)),
        Err(_) => response(
            StatusCode::NOT_FOUND,
            "application/json",
            br#"{"error":"Resource unavailable"}"#.as_slice(),
        ),
    }
}
