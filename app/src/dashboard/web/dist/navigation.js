import { t } from "./i18n.js";
import { describe } from "./i18n.js";
import { workspaceState } from "./state.js";
import { short, $, time, el } from "./ui.js";
import { closePerson } from "./people-directory.js";
import { closeSession } from "./sessions.js";
import { dismissProfile } from "./preferences.js";
import { name } from "./identity.js";
export function roomName(id) {
    const members = workspaceState.data.jobs.filter((j) => j.cohort_id === id), topics = [...new Set(members.flatMap((j) => j.topics || []))], titles = {
        sdlc: t("navigation.ef59116650"),
        memory: t("navigation.50990d7252"),
        collaboration: t("navigation.9839ae273c"),
        computer: t("navigation.f2a536a9a6"),
    };
    return topics.length === 1
        ? titles[topics[0]] || topics[0]
        : members[0]?.team || short(id);
}
export function showSettingsCategory(category) {
    if (!["people", "executors", "evolution", "observation", "service"].includes(category))
        category = workspaceState.settingsCategory;
    if (category !== "people" &&
        typeof closePerson === "function" &&
        $("person-window").classList.contains("embedded"))
        closePerson();
    workspaceState.settingsCategory = category;
    for (const id of [
        "people",
        "executors",
        "evolution",
        "observation",
        "service",
    ]) {
        $("settings-" + id).hidden = id !== category;
        if (id === category)
            $("settings-nav-" + id).setAttribute("aria-current", "page");
        else
            $("settings-nav-" + id).removeAttribute("aria-current");
    }
}
export function showPage(route, focus = false) {
    let [page, category] = route.split("/");
    if (!["collaboration", "messages", "settings"].includes(page))
        page = "collaboration";
    if (page === "settings")
        showSettingsCategory(category);
    else if (typeof closePerson === "function" &&
        $("person-window").classList.contains("embedded"))
        closePerson();
    for (const id of ["collaboration", "messages", "settings"]) {
        $(id + "-page").hidden = id !== page;
        if (id === page)
            $("nav-" + id).setAttribute("aria-current", "page");
        else
            $("nav-" + id).removeAttribute("aria-current");
    }
    if (page !== "collaboration") {
        if ($("session-window").open)
            closeSession();
        dismissProfile();
        if (focus)
            $(page + "-title").focus({ preventScroll: true });
    }
}
export function renderSystemMessages() {
    const current = new Map(), control = workspaceState.data?.controller || {};
    for (const [key, text] of workspaceState.serviceAlerts)
        current.set(key, text);
    if (workspaceState.snapshotError)
        current.set("snapshot-request", workspaceState.snapshotError);
    if (workspaceState.data?.error)
        current.set("snapshot", describe(workspaceState.data.error));
    for (const j of workspaceState.data?.jobs || []) {
        if (j.postprocessing?.persona_memory_error)
            current.set(`persona-memory:${j.run_id}`, t("navigation.24c76134ac", {
                p0: name(j),
                p1: j.postprocessing.persona_memory_error,
            }));
    }
    if (workspaceState.data && !workspaceState.connected)
        current.set("connection", t("navigation.cda4876da0"));
    if (workspaceState.data && !control.live)
        current.set("controller", t("navigation.a3249dcae4"));
    else if (control.stale)
        current.set("heartbeat", t("navigation.80c55d4865", { p0: control.heartbeat_age_seconds }));
    if (control.seed?.error)
        current.set("seed", t("navigation.ac40308658", {
            p0: control.seed.error,
            p1: control.seed.next_attempt_at
                ? t("navigation.ec3be46099") + time(control.seed.next_attempt_at)
                : "",
        }));
    for (const warning of workspaceState.data?.warnings || [])
        current.set("warning:" + JSON.stringify(warning), describe(warning));
    const visibleCurrent = new Map([...current].slice(0, 100));
    const now = Date.now();
    for (const [key, entry] of workspaceState.systemEntries) {
        if (current.has(key) && !visibleCurrent.has(key)) {
            workspaceState.systemEntries.delete(key);
            continue;
        }
        if (entry.active && !current.has(key)) {
            entry.active = false;
            entry.changedAt = now;
        }
    }
    for (const [key, text] of visibleCurrent) {
        const old = workspaceState.systemEntries.get(key);
        if (!old || !old.active)
            workspaceState.systemEntries.set(key, {
                text,
                active: true,
                changedAt: now,
            });
        else
            old.text = text;
    }
    const entries = [...workspaceState.systemEntries].sort(([, a], [, b]) => Number(b.active) - Number(a.active) || b.changedAt - a.changedAt);
    for (const [key] of entries.slice(100))
        workspaceState.systemEntries.delete(key);
    const active = current.size;
    $("system-unread").hidden = !active;
    $("system-unread").textContent = active;
    $("system-message-count").textContent = active
        ? t("navigation.60c9e3b2cc", {
            p0: active,
            p1: active > 100 ? t("navigation.6d144ceaa3") : "",
        })
        : t("navigation.12648f55b2");
    const key = JSON.stringify(entries.slice(0, 100));
    if (workspaceState.systemKey === key)
        return;
    workspaceState.systemKey = key;
    const root = $("system-messages");
    root.replaceChildren();
    for (const [, entry] of entries.slice(0, 100)) {
        const card = el("article", undefined, `system-message ${entry.active ? "active" : "resolved"}`), heading = el("div", undefined, "system-message-heading");
        heading.append(el("strong", entry.active ? t("navigation.f121ab742c") : t("navigation.3617f737f4")), el("time", new Date(entry.changedAt).toLocaleString("zh-CN")));
        card.append(heading, el("p", entry.text));
        root.append(card);
    }
    if (!entries.length)
        root.append(el("div", t("navigation.7e3a9fe2c5"), "empty"));
}
