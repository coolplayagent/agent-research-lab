//! Built frontend modules; TypeScript is checked and regenerated through Bazel.
use crate::transport::response;
use axum::{http::StatusCode, response::Response};
pub(crate) fn contains(path: &str) -> bool {
    asset(path).is_some()
}
fn asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    if let Some(asset) = web_assets::get(path) {
        return Some(asset);
    }
    match path {
        "/" => Some((
            "text/html; charset=utf-8",
            include_bytes!("../web/index.html").as_slice(),
        )),
        "/style.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../web/style.css").as_slice(),
        )),
        "/assets/api.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/api.js").as_slice(),
        )),
        "/assets/applications.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/applications.js").as_slice(),
        )),
        "/assets/bootstrap.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/bootstrap.js").as_slice(),
        )),
        "/assets/conversation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/conversation.js").as_slice(),
        )),
        "/assets/directory.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/directory.js").as_slice(),
        )),
        "/assets/dom.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/dom.js").as_slice(),
        )),
        "/assets/goal-form.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/goal-form.js").as_slice(),
        )),
        "/assets/goal-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/goal-types.js").as_slice(),
        )),
        "/assets/goals.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/goals.js").as_slice(),
        )),
        "/assets/groups.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/groups.js").as_slice(),
        )),
        "/assets/i18n.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/i18n.js").as_slice(),
        )),
        "/assets/locales/zh-CN.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/locales/zh-CN.js").as_slice(),
        )),
        "/assets/members.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/members.js").as_slice(),
        )),
        "/assets/messages.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/messages.js").as_slice(),
        )),
        "/assets/navigation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/navigation.js").as_slice(),
        )),
        "/assets/people.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/people.js").as_slice(),
        )),
        "/assets/picker.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/picker.js").as_slice(),
        )),
        "/assets/types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/types.js").as_slice(),
        )),
        "/assets/service-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/service-forms.js").as_slice(),
        )),
        "/assets/settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/settings.js").as_slice(),
        )),
        "/assets/service-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/service-types.js").as_slice(),
        )),
        "/assets/topics.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/topics.js").as_slice(),
        )),
        "/assets/evolution-render.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/evolution-render.js").as_slice(),
        )),
        "/assets/research-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/research-types.js").as_slice(),
        )),
        "/assets/evolution.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/evolution.js").as_slice(),
        )),
        "/assets/forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/forms.js").as_slice(),
        )),
        "/assets/research-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../web/dist/research-forms.js").as_slice(),
        )),
        _ => None,
    }
}
pub(crate) fn get(path: &str) -> Response {
    match asset(path) {
        Some((mime, body)) => response(StatusCode::OK, mime, body),
        None => response(
            StatusCode::NOT_FOUND,
            "application/json",
            br#"{"error":"Resource unavailable"}"#.as_slice(),
        ),
    }
}
