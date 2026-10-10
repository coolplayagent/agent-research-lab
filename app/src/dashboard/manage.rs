//! Bounded same-origin profile management. No task execution or arbitrary commands.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Profile {
        change: runtime::people::Change,
    },
    Recall {
        id: String,
        query: String,
    },
    Remember {
        id: String,
        request_id: String,
        note: String,
    },
}
#[cfg(test)]
pub(super) fn content_length(headers: &str, addr: SocketAddr) -> Result<usize> {
    let rest = headers
        .strip_prefix("POST /api/people HTTP/1.1\r\n")
        .context("unsupported mutation")?;
    // Reuse the read boundary for Host, Origin and Fetch-Site validation.
    allowed_request(&format!("GET /api/people HTTP/1.1\r\n{rest}"), addr)?;
    let mut fields = BTreeMap::new();
    for line in rest.split("\r\n").filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').context("invalid header")?;
        ensure!(
            fields
                .insert(key.to_ascii_lowercase(), value.trim())
                .is_none(),
            "duplicate request header"
        );
    }
    ensure!(
        fields.contains_key("origin")
            && fields.get("content-type") == Some(&"application/json")
            && fields.get("x-crystal-intent") == Some(&"manage-people"),
        "same-origin JSON management request required"
    );
    ensure!(
        !fields.contains_key("transfer-encoding"),
        "chunked requests unsupported"
    );
    let length: usize = fields
        .get("content-length")
        .context("content length required")?
        .parse()?;
    ensure!(
        (1..=16 * 1024).contains(&length),
        "profile request exceeds bound"
    );
    Ok(length)
}
pub(super) fn apply(c: &Config, body: &[u8]) -> Result<Value> {
    let _deadline = process::deadline_scope(Duration::from_secs(22));
    match serde_json::from_slice::<Request>(body)? {
        Request::Profile { change } => Ok(json!({"person":runtime::people::change(c,change)?})),
        Request::Recall { id, query } => {
            ensure!(query.len() <= 4000, "recall query exceeds bound");
            personas::memory(c, &id, &query)
        }
        Request::Remember {
            id,
            request_id,
            note,
        } => runtime::people::remember_note(c, &id, &request_id, &note),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutations_require_exact_origin_intent_json_and_bounded_length() {
        let addr = "127.0.0.1:8090".parse().unwrap();
        let valid = "POST /api/people HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nOrigin: http://127.0.0.1:8090\r\nContent-Type: application/json\r\nX-Crystal-Intent: manage-people\r\nContent-Length: 24\r\n\r\n";
        assert_eq!(content_length(valid, addr).unwrap(), 24);
        for request in [
            valid.replace("Origin: http://127.0.0.1:8090\r\n", ""),
            valid.replace("application/json", "text/plain"),
            valid.replace("manage-people", "run"),
            valid.replace("Length: 24", "Length: 20000"),
            valid.replace(
                "Origin: http://127.0.0.1:8090",
                "Origin: https://foreign.example",
            ),
            valid.replace(
                "Content-Length: 24",
                "Transfer-Encoding: chunked\r\nContent-Length: 24",
            ),
            valid.replace(
                "Content-Length: 24",
                "Content-Length: 24\r\nContent-Length: 24",
            ),
            valid.replace("/api/people", "/api/enqueue"),
        ] {
            assert!(content_length(&request, addr).is_err());
        }
    }
}
