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
        ? group.kind === "board"
        : section === "direct"
            ? group.private || group.kind === "temporary"
            : group.kind === "conversation" && !group.private;
}
function render() {
    const root = get("group-list");
    root.replaceChildren();
    for (const group of rows.filter(belongs)) {
        const item = button("", () => choose(group));
        item.className = "group-item" + (group.id === selected ? " selected" : "");
        item.append(el("strong", `${group.pinned ? "★ " : ""}${group.title}`), el("small", `${group.archived ? "已归档 · " : ""}${group.member_count} 位成员`));
        root.append(item);
    }
    if (!root.childElementCount)
        root.append(el("p", "暂无会话，点击“发起会话”开始。", "empty"));
}
export function metrics(data) {
    const root = get("metrics");
    root.replaceChildren();
    for (const [label, key] of [
        ["Agent", "actors"],
        ["群组与会话", "groups"],
        ["持久消息", "stored_messages"],
        ["实时连接", "subscriptions"],
        ["待写入消息", "queue_depth"],
        ["重放消息", "replayed"],
        ["背压拒绝", "rejected"],
    ])
        root.append(el("dt", label), el("dd", String(data[key] ?? "—")));
    root.append(el("dt", "存储状态"), el("dd", data.healthy ? "正常" : "等待恢复"));
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
