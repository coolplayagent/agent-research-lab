import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice, value } from "./dom.js";
let section = "groups", next = null, rows = [], selected = "", generation = 0;
let choose;
export function setSection(value) {
    section = value;
    render();
}
export function setSelected(id) {
    selected = id;
    render();
}
function belongs(group) {
    return section === "boards"
        ? group.kind === GroupKind.Board
        : section === "direct"
            ? group.private || group.kind === GroupKind.Temporary
            : group.kind === GroupKind.Conversation && !group.private;
}
function render() {
    const root = get("group-list");
    root.replaceChildren();
    for (const group of rows.filter(belongs)) {
        const item = button("", () => choose(group));
        item.className = "group-item" + (group.id === selected ? " selected" : "");
        item.append(el("strong", `${group.pinned ? "★ " : ""}${group.title}`), el("small", t("directory.c04a113aca", {
            p0: group.archived ? t("directory.bafe86d881") : "",
            p1: group.member_count,
        })));
        root.append(item);
    }
    if (!root.childElementCount)
        root.append(el("p", t("directory.49ff4c1834"), "empty"));
}
export function metrics(data) {
    const root = get("metrics");
    root.replaceChildren();
    for (const [label, key] of [
        ["Agent", "actors"],
        [t("directory.0a29c2ff50"), "groups"],
        [t("directory.4cc404a451"), "stored_messages"],
        [t("directory.a1b56411cc"), "subscriptions"],
        [t("directory.e1fa4a08fc"), "queue_depth"],
        [t("directory.56eb5b282c"), "replayed"],
        [t("directory.1e55edfca7"), "rejected"],
    ])
        root.append(el("dt", label), el("dd", String(data[key] ?? "—")));
    root.append(el("dt", t("directory.f0f4fa0c46")), el("dd", data.healthy ? t("directory.296de0e31f") : t("directory.20f8e5ca6d")));
}
export async function loadGroups(append = false) {
    const request = ++generation;
    try {
        const result = await api("/api/crystal/view?" +
            query({
                after: append ? next || "" : "",
                query: value("group-search"),
            }));
        if (generation !== request)
            return;
        rows = append
            ? [...rows, ...result.directory.groups]
            : result.directory.groups;
        next = result.directory.next_after;
        get("more-groups").hidden = !next;
        metrics(result.metrics);
        render();
    }
    catch (error) {
        notice(error);
    }
}
export function initDirectory(select) {
    choose = select;
    get("search-groups").onclick = () => void loadGroups();
    get("more-groups").onclick = () => void loadGroups(true);
}
