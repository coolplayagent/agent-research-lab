//! Embedded console frontend assets.
pub fn get(path: &str) -> Option<(&'static str, &'static [u8])> {
    match path {
        "/apps/console/" => Some((
            "text/html; charset=utf-8",
            include_bytes!("../index.html").as_slice(),
        )),
        "/apps/console/style.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../style.css").as_slice(),
        )),
        "/apps/console/assets/bootstrap.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/bootstrap.js").as_slice(),
        )),
        "/apps/console/assets/channels.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/channels.js").as_slice(),
        )),
        "/apps/console/assets/events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/events.js").as_slice(),
        )),
        "/apps/console/assets/executors.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/executors.js").as_slice(),
        )),
        "/apps/console/assets/graph.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/graph.js").as_slice(),
        )),
        "/apps/console/assets/i18n.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/i18n.js").as_slice(),
        )),
        "/apps/console/assets/identity.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/identity.js").as_slice(),
        )),
        "/apps/console/assets/lineage-events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/lineage-events.js").as_slice(),
        )),
        "/apps/console/assets/lineage.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/lineage.js").as_slice(),
        )),
        "/apps/console/assets/locales/zh-CN.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/locales/zh-CN.js").as_slice(),
        )),
        "/apps/console/assets/navigation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/navigation.js").as_slice(),
        )),
        "/apps/console/assets/observer.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/observer.js").as_slice(),
        )),
        "/apps/console/assets/people-directory.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-directory.js").as_slice(),
        )),
        "/apps/console/assets/people-events.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-events.js").as_slice(),
        )),
        "/apps/console/assets/people-profile.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people-profile.js").as_slice(),
        )),
        "/apps/console/assets/preferences.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/preferences.js").as_slice(),
        )),
        "/apps/console/assets/process.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/process.js").as_slice(),
        )),
        "/apps/console/assets/profile.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/profile.js").as_slice(),
        )),
        "/apps/console/assets/sessions.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/sessions.js").as_slice(),
        )),
        "/apps/console/assets/sizing.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/sizing.js").as_slice(),
        )),
        "/apps/console/assets/state.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/state.js").as_slice(),
        )),
        "/apps/console/assets/ui.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/ui.js").as_slice(),
        )),
        _ => None,
    }
}
