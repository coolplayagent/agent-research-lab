//! Shared frontend assets, kept in their own package for hermetic embedding.
pub fn get(path: &str) -> Option<(&'static str, &'static [u8])> {
    let body = match path {
        "/assets/shared/contracts.js" => include_bytes!("../dist/contracts.js").as_slice(),
        "/assets/shared/i18n.js" => include_bytes!("../dist/i18n.js").as_slice(),
        "/assets/shared/locales/zh-CN.js" => include_bytes!("../dist/locales/zh-CN.js").as_slice(),
        _ => return None,
    };
    Some(("text/javascript; charset=utf-8", body))
}
