import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice, value } from "./dom.js";
let next = null, rows = [], selected = "", generation = 0;
let choose;
let restored = false;
export function setSelected(id) {
    selected = id;
    sessionStorage.setItem("workspace-group", id);
    render();
}
function render() {
    const root = get("group-list");
    const folded = new Set([...root.querySelectorAll("details:not([open])")].map((node) => node.dataset.section));
    root.replaceChildren();
    const sections = [
        [
            "groups",
            t("workspace.groups"),
            (g) => g.kind === GroupKind.Conversation && !g.private,
        ],
        [
            "direct",
            t("workspace.direct"),
            (g) => g.kind !== GroupKind.Board &&
                (g.private || g.kind === GroupKind.Temporary),
        ],
        ["boards", t("workspace.boards"), (g) => g.kind === GroupKind.Board],
    ];
    for (const [id, title, belongs] of sections) {
        const section = el("details", undefined, "directory-section");
        section.dataset.section = id;
        section.open = !folded.has(id);
        section.append(el("summary", title));
        for (const group of rows
            .filter(belongs)
            .sort((a, b) => Number(b.pinned) - Number(a.pinned))) {
            const item = button("", () => {
                location.hash = id;
                choose(group);
                get("workspace").classList.remove("directory-open");
            });
            item.className =
                "group-item" + (group.id === selected ? " selected" : "");
            item.title = group.title;
            item.setAttribute("aria-current", group.id === selected ? "true" : "false");
            item.append(el("span", group.pinned
                ? "★"
                : id === "groups"
                    ? "#"
                    : id === "boards"
                        ? "◇"
                        : "@"), el("strong", group.title), el("small", group.archived
                ? t("directory.bafe86d881")
                : String(group.member_count)));
            section.append(item);
        }
        root.append(section);
    }
    if (!rows.length)
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
        if (!restored) {
            restored = true;
            const saved = sessionStorage.getItem("workspace-group");
            const found = rows.find((g) => g.id === saved);
            if (found)
                choose(found);
            else if (saved) {
                try {
                    const view = await api("/api/crystal/view?" + query({ group_id: saved }));
                    if (generation === request && !selected)
                        choose(view.group);
                }
                catch {
                    if (!selected && rows[0])
                        choose(rows[0]);
                }
            }
            else if (rows[0])
                choose(rows[0]);
        }
    }
    catch (error) {
        notice(error);
    }
}
export function initDirectory(select) {
    choose = select;
    get("group-search").addEventListener("keydown", (event) => {
        if (event.key === "Enter")
            void loadGroups();
    });
    get("search-groups").onclick = () => void loadGroups();
    get("more-groups").onclick = () => void loadGroups(true);
}
