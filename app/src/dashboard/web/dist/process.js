import { SessionItemStatus } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { workspaceState } from "./state.js";
import { fold, readable, name } from "./identity.js";
import { el } from "./ui.js";
export function isError(e) {
    return (["turn.failed", "error"].includes(e.type) ||
        [SessionItemStatus.Failed, SessionItemStatus.Error].includes(e.status) ||
        (e.exit_code != null && e.exit_code !== 0));
}
export function eventDetails(j, e) {
    const id = `${j.run_id}:${e.id}`, row = fold(id, `${isError(e) ? "! " : ""}${label("SessionEventKind", e.title)}${e.exit_code != null ? " · exit " + e.exit_code : ""}`, "process-event");
    if (e.text)
        row.append(readable(e.text, `${id}:text`));
    if (e.command)
        row.append(el("pre", e.command));
    if (e.output)
        row.append(el("pre", e.output));
    const raw = fold(`${id}:json`, t("process.4abbdf59f7"), "protocol-detail");
    raw.append(el("pre", JSON.stringify(e, null, 2)));
    row.append(raw);
    return row;
}
export function processBlock(j, s) {
    const block = el("div", undefined, "session-transcript"), body = el("div", undefined, "process-body"), events = s?.events || [];
    if (!s)
        body.append(el("p", t("process.5e3cf44431"), "empty"));
    else if (!events.length)
        body.append(el("p", t("process.666b6209f5"), "empty"));
    let tools = [];
    const flush = () => {
        if (!tools.length)
            return;
        const failed = tools.filter(isError).length, group = fold(`tools:${j.run_id}:${tools[0].id}`, t("process.dfc6236d09", {
            p0: tools.length,
            p1: failed ? t("process.c414cf78f7", { p0: failed }) : "",
        }), "tool-group");
        group.firstChild.append(el("small", label("SessionEventKind", tools.at(-1).title)));
        if (failed) {
            group.classList.add("has-errors");
            body.append(el("p", t("process.0bdcf3e572"), "activity-error"));
        }
        tools.forEach((e) => group.append(eventDetails(j, e)));
        body.append(group);
        tools = [];
    };
    for (const e of events) {
        const speaking = e.item_type === "agent_message" ||
            (e.type === "session.activity" && e.text);
        if (speaking) {
            flush();
            const message = el("article", undefined, "bot-message");
            message.append(el("div", name(j), "bot-label"));
            let structured = null;
            try {
                structured = JSON.parse(e.text);
            }
            catch (_) { }
            const text = typeof structured?.summary === "string"
                ? structured.summary
                : e.text || t("process.9d49394a45");
            message.append(readable(text, `${j.run_id}:${e.id}:message`));
            const raw = fold(`${j.run_id}:${e.id}:json`, t("process.6ed3b11550"), "protocol-detail");
            raw.append(el("pre", JSON.stringify(e, null, 2)));
            message.append(raw);
            body.append(message);
        }
        else if (["thread.started", "turn.started", "turn.completed"].includes(e.type)) {
            flush();
            body.append(el("div", label("SessionEventKind", e.title), "lifecycle"));
        }
        else if (["turn.failed", "error"].includes(e.type)) {
            flush();
            body.append(el("div", e.text || label("SessionEventKind", e.title), "activity-error"), eventDetails(j, e));
        }
        else
            tools.push(e);
    }
    flush();
    if (workspaceState.historyState?.following && j.report?.summary) {
        const result = el("article", undefined, "bot-message result-message");
        result.append(el("div", t("process.1051b21266"), "bot-label"), readable(j.report.summary, `result:${j.run_id}`));
        const more = fold(`result-details:${j.run_id}`, t("process.a4b4a34a30"), "message-more");
        for (const text of j.report.findings || [])
            if (text)
                more.append(el("p", text));
        for (const text of j.report.limitations || [])
            if (text)
                more.append(el("p", t("process.4d7ab217a4") + text));
        result.append(more);
        body.append(result);
    }
    body.append(el("p", t("process.0c14371179"), "process-meta"));
    if (s?.invalid_lines)
        body.append(el("p", t("process.f0c45a1b12", { p0: s.invalid_lines }), "process-meta"));
    block.append(body);
    return block;
}
