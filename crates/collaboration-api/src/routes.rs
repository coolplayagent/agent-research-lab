//! Canonical route identities shared by compatibility authorization.
use std::borrow::Cow;
pub fn legacy_path(path: &str) -> Cow<'_, str> {
    if path == "/api/v1/agent/outcomes" {
        return Cow::Borrowed("/api/crystal/outcomes");
    }
    if path == "/api/v1/collaboration/coordination" {
        return Cow::Borrowed("/api/im/research");
    }
    for (new, old) in [
        ("/api/v1/collaboration/", "/api/im/"),
        ("/api/v1/messaging/", "/api/crystal/"),
    ] {
        if let Some(suffix) = path.strip_prefix(new) {
            return Cow::Owned(format!("{old}{suffix}"));
        }
    }
    Cow::Borrowed(path)
}
