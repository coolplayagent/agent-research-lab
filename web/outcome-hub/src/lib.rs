//! Embedded OutcomeHub frontend assets.
pub fn get(path: &str) -> Option<(&'static str, &'static [u8])> {
    if let Some(asset) = web_assets::get(path) {
        return Some(asset);
    }
    match path {
        "/assets/adapter-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/adapter-forms.js").as_slice(),
        )),
        "/assets/execution-status.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/execution-status.js").as_slice(),
        )),
        "/assets/binding-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/binding-types.js").as_slice(),
        )),
        "/assets/binding-settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/binding-settings.js").as_slice(),
        )),
        "/assets/binding-progress.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/binding-progress.js").as_slice(),
        )),

        "/assets/outcome-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/outcome-types.js").as_slice(),
        )),
        "/assets/outcome-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/outcome-forms.js").as_slice(),
        )),
        "/assets/outcome-detail.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/outcome-detail.js").as_slice(),
        )),
        "/assets/outcome-board.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/outcome-board.js").as_slice(),
        )),
        "/assets/outcome-settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/outcome-settings.js").as_slice(),
        )),

        "/assets/scenario-catalog.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/scenario-catalog.js").as_slice(),
        )),
        "/assets/iteration-review.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/iteration-review.js").as_slice(),
        )),
        "/assets/iteration-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/iteration-types.js").as_slice(),
        )),
        "/assets/iteration-render.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/iteration-render.js").as_slice(),
        )),
        "/assets/iteration-form.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/iteration-form.js").as_slice(),
        )),
        "/assets/iterations.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/iterations.js").as_slice(),
        )),
        "/assets/scenario-settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/scenario-settings.js").as_slice(),
        )),
        "/" => Some((
            "text/html; charset=utf-8",
            include_bytes!("../index.html").as_slice(),
        )),
        "/favicon.svg" => Some(("image/svg+xml", include_bytes!("../favicon.svg").as_slice())),
        "/assets/experience.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/experience.js").as_slice(),
        )),
        "/assets/feedback.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/feedback.js").as_slice(),
        )),
        "/style.css" => Some((
            "text/css; charset=utf-8",
            include_bytes!("../style.css").as_slice(),
        )),
        "/assets/workspace.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/workspace.js").as_slice(),
        )),
        "/assets/settings-guard.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/settings-guard.js").as_slice(),
        )),
        "/assets/collaboration.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/collaboration.js").as_slice(),
        )),
        "/assets/collaboration-graph.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/collaboration-graph.js").as_slice(),
        )),
        "/assets/collaboration-model.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/collaboration-model.js").as_slice(),
        )),
        "/assets/api.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/api.js").as_slice(),
        )),
        "/assets/applications.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/applications.js").as_slice(),
        )),
        "/assets/bootstrap.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/bootstrap.js").as_slice(),
        )),
        "/assets/conversation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/conversation.js").as_slice(),
        )),
        "/assets/directory.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/directory.js").as_slice(),
        )),
        "/assets/dom.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/dom.js").as_slice(),
        )),
        "/assets/goal-form.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/goal-form.js").as_slice(),
        )),
        "/assets/goal-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/goal-types.js").as_slice(),
        )),
        "/assets/goal-selection.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/goal-selection.js").as_slice(),
        )),
        "/assets/goals.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/goals.js").as_slice(),
        )),
        "/assets/groups.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/groups.js").as_slice(),
        )),
        "/assets/i18n.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/i18n.js").as_slice(),
        )),
        "/assets/locales/zh-CN.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/locales/zh-CN.js").as_slice(),
        )),
        "/assets/members.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/members.js").as_slice(),
        )),
        "/assets/message-results.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/message-results.js").as_slice(),
        )),
        "/assets/message-drafts.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/message-drafts.js").as_slice(),
        )),
        "/assets/contribution-text.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/contribution-text.js").as_slice(),
        )),
        "/assets/mentions.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/mentions.js").as_slice(),
        )),
        "/assets/mention-model.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/mention-model.js").as_slice(),
        )),
        "/assets/avatar.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/avatar.js").as_slice(),
        )),
        "/assets/message-view.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/message-view.js").as_slice(),
        )),
        "/assets/research-process.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/research-process.js").as_slice(),
        )),
        "/assets/task-progress.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/task-progress.js").as_slice(),
        )),
        "/assets/icons.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/icons.js").as_slice(),
        )),
        "/assets/messages.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/messages.js").as_slice(),
        )),
        "/assets/navigation.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/navigation.js").as_slice(),
        )),
        "/assets/people.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/people.js").as_slice(),
        )),
        "/assets/picker.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/picker.js").as_slice(),
        )),
        "/assets/types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/types.js").as_slice(),
        )),
        "/assets/service-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/service-forms.js").as_slice(),
        )),
        "/assets/settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/settings.js").as_slice(),
        )),
        "/assets/service-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/service-types.js").as_slice(),
        )),
        "/assets/topics.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/topics.js").as_slice(),
        )),
        "/assets/evolution-render.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/evolution-render.js").as_slice(),
        )),
        "/assets/research-types.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/research-types.js").as_slice(),
        )),
        "/assets/evolution.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/evolution.js").as_slice(),
        )),
        "/assets/forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/forms.js").as_slice(),
        )),
        "/assets/research-forms.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/research-forms.js").as_slice(),
        )),
        "/assets/knowledge-settings.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/knowledge-settings.js").as_slice(),
        )),
        "/assets/knowledge-publish.js" => Some((
            "text/javascript; charset=utf-8",
            include_bytes!("../dist/knowledge-publish.js").as_slice(),
        )),
        _ => None,
    }
}
