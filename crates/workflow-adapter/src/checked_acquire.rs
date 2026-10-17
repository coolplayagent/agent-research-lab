//! A one-shot preconditioned acquisition. Ambiguous replies never prove rollback.
use super::*;
use contracts::{AcquirePreconditionsV1, CheckedAcquireFailureCode};

#[derive(Debug)]
pub struct CheckedAcquireError {
    pub code: CheckedAcquireFailureCode,
    pub message: String,
}
impl std::fmt::Display for CheckedAcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for CheckedAcquireError {}

impl Workflow {
    pub fn acquire_checked(
        &self,
        run_id: &str,
        owner: &str,
        acquisition_id: &str,
        ttl_ms: u64,
        expected: &AcquirePreconditionsV1,
    ) -> Result<Value> {
        expected.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            !run_id.is_empty() && !owner.is_empty() && !acquisition_id.is_empty() && ttl_ms > 0,
            "checked acquisition identity or ttl is invalid"
        );
        let mut request = serde_json::to_value(expected)?;
        request["schema_version"] = json!(1);
        request["lease"] = json!({"run_id":run_id,"owner":owner,
            "acquisition_id":acquisition_id,"ttl_ms":ttl_ms});
        let path = self.save("checked-lease-request-v1", &request)?;
        let args = vec!["run".into(), "acquire-checked-v1".into(), self.db(), path];
        let result = (|| -> Result<Value> {
            let output = process::capture(
                self.binary.to_str().context("non-UTF8 workflow binary")?,
                &args,
                &self.work_dir,
                Duration::from_secs(30),
            )?;
            let response = strict_reply(&output.stdout)?;
            ensure!(
                output.stderr.is_empty(),
                "checked acquire emitted unexpected stderr"
            );
            if output.status.code() == Some(1)
                && response["ok"] == Value::Bool(false)
                && contracts::CheckedAcquireFailureCode::from_value(&response["error"]["code"])
                    == Some(CheckedAcquireFailureCode::AdmissionConflict)
            {
                let conflict: ConflictEnvelope = serde_json::from_value(response)
                    .context("checked acquire conflict envelope is ambiguous")?;
                ensure!(
                    !conflict.ok
                        && conflict.error.code == CheckedAcquireFailureCode::AdmissionConflict
                        && conflict.error.kernel_code.is_null()
                        && !conflict.error.message.is_empty(),
                    "checked acquire conflict is incomplete"
                );
                return Err(CheckedAcquireError {
                    code: CheckedAcquireFailureCode::AdmissionConflict,
                    message: conflict.error.message,
                }
                .into());
            }
            ensure!(
                output.status.success(),
                "checked acquire request failed: {response}"
            );
            let response: SuccessEnvelope = serde_json::from_value(response)
                .context("checked acquire success envelope is ambiguous")?;
            ensure!(response.ok, "checked acquire missing success envelope");
            let lease = response.result;
            let authority: contracts::CheckedLeaseAuthority = serde_json::from_value(lease.clone())
                .context("checked acquire lease schema is incomplete")?;
            let now = now_ms()?;
            ensure!(
                authority.run_id == run_id
                    && authority.owner == owner
                    && authority.acquisition_id == acquisition_id
                    && authority.epoch > 0
                    && authority
                        .generation
                        .as_ref()
                        .is_some_and(|g| !g.is_empty() && g.len() <= 256)
                    && authority.issued_at_unix_ms > 0
                    && authority.issued_at_unix_ms <= now
                    && now < authority.expires_at_unix_ms
                    && authority
                        .expires_at_unix_ms
                        .checked_sub(authority.issued_at_unix_ms)
                        == Some(ttl_ms),
                "checked acquire returned different lease authority"
            );
            Ok(lease)
        })();
        result.map_err(|error| {
            if error.is::<CheckedAcquireError>() {
                error
            } else {
                CheckedAcquireError {
                    code: CheckedAcquireFailureCode::Unknown,
                    message: format!("{error:#}"),
                }
                .into()
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuccessEnvelope {
    ok: bool,
    result: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConflictEnvelope {
    ok: bool,
    error: ConflictBody,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConflictBody {
    code: CheckedAcquireFailureCode,
    message: String,
    kernel_code: Value,
}

fn strict_reply(bytes: &[u8]) -> Result<Value> {
    use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
    struct Strict(Value);
    struct JsonVisitor;
    impl<'de> Deserialize<'de> for Strict {
        fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            d.deserialize_any(JsonVisitor)
        }
    }
    impl<'de> Visitor<'de> for JsonVisitor {
        type Value = Strict;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("one JSON value without duplicate keys")
        }
        fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Strict, E> {
            Ok(Strict(json!(v)))
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Strict, E> {
            Ok(Strict(Value::Null))
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Strict, A::Error> {
            let mut values = Vec::new();
            while let Some(Strict(value)) = a.next_element()? {
                values.push(value);
            }
            Ok(Strict(Value::Array(values)))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Strict, A::Error> {
            let mut values = serde_json::Map::new();
            while let Some(key) = a.next_key::<String>()? {
                if values.contains_key(&key) {
                    return Err(serde::de::Error::custom("duplicate checked reply key"));
                }
                let Strict(value) = a.next_value()?;
                values.insert(key, value);
            }
            Ok(Strict(Value::Object(values)))
        }
    }
    ensure!(
        bytes.len() <= 8192,
        "checked acquire reply exceeds schema cap"
    );
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let Strict(value) = Strict::deserialize(&mut decoder)?;
    decoder.end()?;
    Ok(value)
}

#[cfg(test)]
#[path = "checked_acquire_tests.rs"]
mod tests;
