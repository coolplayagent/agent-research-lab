import { avatar } from "./avatar.js";
import { api, query } from "./api.js";
import { button, el, get, presence } from "./dom.js";
import { t, label } from "./i18n.js";
import { display, people } from "./people.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { messageSnapshot, locateMessage } from "./messages.js";
import { renderCrystal } from "./collaboration-graph.js";
import { revealResearch } from "./workspace.js";
import { insertMention } from "./mentions.js";
let view = null, selected = "", sequence = 0, epoch = 0;
let members = [], after = null;
let bindings = null;
let bindingRequest = 0;
async function refreshBindings() {
    if (!view)
        return;
    const context = epoch, request = ++bindingRequest;
    bindings = null;
    if (selected)
        detail(selected);
    try {
        const result = await api("/api/v1/collaboration/services");
        if (context !== epoch || request !== bindingRequest)
            return;
        bindings = result.configuration.bindings;
        if (selected)
            detail(selected);
    }
    catch {
        /* Availability is unknown after a failed refresh. */
    }
}
let timer;
let pulseTimer;
const pulses = new Map();
export function selectCollaboration(value) {
    sequence++;
    epoch++;
    clearTimeout(timer);
    clearTimeout(pulseTimer);
    pulses.clear();
    view = value;
    selected = "";
    bindings = null;
    members = value?.members.members || [];
    after = value?.members.next_after || null;
    get("collaboration-detail").replaceChildren();
    render();
    if (value)
        void refreshBindings();
}
export function refreshCollaboration() {
    if (!view)
        return;
    clearTimeout(timer);
    timer = setTimeout(() => void loadMembers(false), 150);
}
async function loadMembers(more) {
    if (!view)
        return;
    const request = ++sequence, id = view.group.id;
    try {
        let cursor = more ? after || "" : "";
        const target = more ? 0 : members.length;
        let loaded = [];
        do {
            const page = await api("/api/v1/messaging/view?" + query({ group_id: id, after: cursor }));
            if (request !== sequence)
                return;
            view = page;
            loaded.push(...page.members.members);
            const next = page.members.next_after || "";
            if (next === cursor) {
                cursor = "";
                break;
            }
            cursor = next;
        } while (!more && cursor && loaded.length < target);
        members = more ? [...members, ...loaded] : loaded;
        after = cursor || null;
        render();
    }
    catch (error) {
        if (request === sequence)
            get("collaboration-status").textContent = String(error);
    }
}
function detail(id) {
    selected = id;
    const member = members.find((m) => m.person_id === id);
    if (!member) {
        get("collaboration-detail").replaceChildren();
        return;
    }
    const panel = get("collaboration-detail");
    panel.replaceChildren(avatar(id), button(`@${display(id)}`, () => insertMention(id)), el("p", presence(member.presence)));
    if (id !== "operator") {
        panel.append(el("p", bindings === null
            ? t("workspace.bindingUnknown")
            : bindings[id]
                ? t("workspace.bound", { adapter: bindings[id] })
                : t("workspace.unbound")));
        const link = el("a", t("workspace.configureExecution"));
        link.href = "#settings/bindings";
        panel.append(link);
    }
    for (const goal of goalSnapshot()) {
        for (const work of goal.work.filter((work) => work.person_id === id)) {
            panel.append(button(goal.input.title, () => focusGoal(goal.input.id)), el("p", `${label("WorkState", work.state)} · ${work.instruction}`));
        }
    }
    const message = messageSnapshot()
        .filter((m) => m.sender_id === id)
        .at(-1);
    if (message)
        panel.append(button(t("workspace.locateMessage"), () => {
            get("workspace").classList.remove("research-expanded", "research-open");
            locateMessage(message.sequence);
        }));
}
function render() {
    const host = get("collaboration-graph"), list = get("research-members");
    host.replaceChildren();
    list.replaceChildren();
    get("more-collaboration").hidden = !after;
    get("collaboration-status").textContent = view
        ? t("workspace.loadedMembers", {
            count: members.length,
            total: view.group.member_count,
        })
        : t("workspace.selectGroup");
    if (!view)
        return;
    renderCrystal(host, view.group, members, messageSnapshot(), goalSnapshot(), detail, new Set([...pulses].filter(([, until]) => until > Date.now()).map(([id]) => id)), () => { get("workspace").classList.remove("research-expanded", "research-open"); get("message-text").focus(); });
    for (const member of members) {
        const row = el("div", undefined, "member");
        row.append(avatar(member.person_id), button(display(member.person_id), () => {
            revealResearch("collaboration");
            detail(member.person_id);
        }), el("small", presence(member.presence)));
        list.append(row);
    }
    if (after)
        list.append(button(t("html.ae4ed171f5"), () => void loadMembers(true)));
    if (selected)
        detail(selected);
}
export function initCollaboration() {
    get("more-collaboration").onclick = () => void loadMembers(true);
    get("refresh-collaboration").onclick = () => {
        void loadMembers(false);
        void refreshBindings();
    };
    document.addEventListener("workspace-services", () => void refreshBindings());
    document.addEventListener("workspace-message", (event) => {
        const message = event.detail;
        if (message.group_id !== view?.group.id || (message.event && message.event.person_id !== message.sender_id))
            return;
        pulses.set(message.sender_id, Date.now() + 1600);
        clearTimeout(pulseTimer);
        render();
        pulseTimer = setTimeout(() => { pulses.clear(); render(); }, 1650);
    });
    let width = 0;
    new ResizeObserver(([entry]) => {
        const next = Math.round(entry.contentRect.width);
        if (next && Math.abs(next - width) > 10) {
            width = next;
            render();
        }
    }).observe(get("collaboration-graph"));
    document.addEventListener("workspace-messages", render);
    document.addEventListener("workspace-goals", render);
    document.addEventListener("workspace-people", render);
    void (async () => {
        let cursor = "", count = 0;
        do {
            const page = await api("/api/v1/collaboration/people?" + query({ after: cursor }));
            for (const person of page.people)
                people.set(person.id, person);
            const next = page.next_after || "";
            if (next === cursor)
                break;
            cursor = next;
            count += page.people.length;
        } while (cursor && count < 10000);
        render();
    })().catch(() => { });
}
