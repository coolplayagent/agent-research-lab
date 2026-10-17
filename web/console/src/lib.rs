//! Embedded console frontend assets.
pub fn get(path: &str) -> Option<(&'static str, &'static [u8])> {
    let legacy;
    let path = if let Some(suffix) = path.strip_prefix("/apps/console/") {
        legacy = format!("/apps/research/{suffix}");
        legacy.as_str()
    } else {
        path
    };
    match path {
        "/apps/research/" => Some((
            "text/html; charset=utf-8",
            include_bytes!("../index.html").as_slice(),
        )),
        "/apps/research/style.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../style.css").as_slice(),
        )),
        "/apps/research/assets/bootstrap.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/bootstrap.js").as_slice(),
        )),
        "/apps/research/assets/channels.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/channels.js").as_slice(),
        )),
        "/apps/research/assets/events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/events.js").as_slice(),
        )),
        "/apps/research/assets/executors.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/executors.js").as_slice(),
        )),
        "/apps/research/assets/graph.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/graph.js").as_slice(),
        )),
        "/apps/research/assets/i18n.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/i18n.js").as_slice(),
        )),
        "/apps/research/assets/identity.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/identity.js").as_slice(),
        )),
        "/apps/research/assets/lineage-events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/lineage-events.js").as_slice(),
        )),
        "/apps/research/assets/lineage.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/lineage.js").as_slice(),
        )),
        "/apps/research/assets/locales/zh-CN.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/locales/zh-CN.js").as_slice(),
        )),
        "/apps/research/assets/navigation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/navigation.js").as_slice(),
        )),
        "/apps/research/assets/observer.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/observer.js").as_slice(),
        )),
        "/apps/research/assets/people-directory.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-directory.js").as_slice(),
        )),
        "/apps/research/assets/people-events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-events.js").as_slice(),
        )),
        "/apps/research/assets/people-profile.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-profile.js").as_slice(),
        )),
        "/apps/research/assets/preferences.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/preferences.js").as_slice(),
        )),
        "/apps/research/assets/process.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/process.js").as_slice(),
        )),
        "/apps/research/assets/profile.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/profile.js").as_slice(),
        )),
        "/apps/research/assets/sessions.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/sessions.js").as_slice(),
        )),
        "/apps/research/assets/sizing.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/sizing.js").as_slice(),
        )),
        "/apps/research/assets/state.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/state.js").as_slice(),
        )),
        "/apps/research/assets/ui.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/ui.js").as_slice(),
        )),
        _ => None,
    }
}
