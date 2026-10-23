import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice, value } from "./dom.js";
import { searchKeys } from "./experience.js";
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
    get("directory-count").textContent = t("ux.directoryCount", { count: rows.length });
    if (!rows.length) {
        const empty = el("div", undefined, "directory-empty");
        empty.append(el("strong", value("group-search") ? t("ux.noResults") : t("directory.49ff4c1834")));
        if (value("group-search")) {
            empty.append(el("p", t("ux.noResultsNote")), button(t("ux.clearSearch"), () => {
                get("group-search").value = "";
                return loadGroups();
            }));
        }
        else
            empty.append(button(t("ux.createConversation"), () => get("new-conversation").click()));
        root.append(empty);
    }
}
export async function loadGroups(append = false) {
    const request = ++generation;
    get("group-list").setAttribute("aria-busy", "true");
    if (!rows.length)
        get("directory-count").textContent = t("ux.loading");
    try {
        const result = await api("/api/v1/messaging/view?" +
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
        render();
        if (!restored) {
            restored = true;
            if (/^#(?:groups|tasks|outcomes)\//.test(location.hash))
                return;
            const saved = sessionStorage.getItem("workspace-group");
            const found = rows.find((g) => g.id === saved);
            if (found)
                choose(found);
            else if (saved) {
                try {
                    const view = await api("/api/v1/messaging/view?" + query({ group_id: saved }));
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
        if (generation === request) {
            notice(error);
            get("directory-count").textContent = t("ux.searchFailed");
        }
    }
    finally {
        if (generation === request)
            get("group-list").removeAttribute("aria-busy");
    }
}
export function initDirectory(select) {
    choose = select;
    searchKeys(get("group-search"), () => void loadGroups());
    get("search-groups").onclick = () => void loadGroups();
    get("more-groups").onclick = () => void loadGroups(true);
}
