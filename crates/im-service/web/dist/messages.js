import { GroupKind, GoalEventKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { resultForMessage } from "./message-results.js";
import { api, change, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
import { MessageDrafts } from "./message-drafts.js";
let group = null, generation = 0, browsing = false, reply = null;
let historyGeneration = 0, newestSequence = 0, unseen = 0;
const messages = new Map();
const liveMessages = new Map();
const drafts = new MessageDrafts();
export function messageSnapshot() {
    return [...messages.values()];
}
export function suspendMessages() {
    if (group)
        drafts.save(group.id, {
            text: get("message-text").value,
            reply,
        });
    group = null;
    generation++;
}
function sizeComposer() {
    const input = get("message-text");
    input.style.height = "46px";
    input.style.height = Math.min(130, Math.max(46, input.scrollHeight)) + "px";
}
export function locateMessage(sequence) {
    for (const row of get("messages").querySelectorAll("[data-sequence]")) {
        row.classList.toggle("target", row.dataset.sequence === String(sequence));
        if (row.dataset.sequence === String(sequence))
            row.scrollIntoView({ block: "center" });
    }
}
export function resetMessages(selected, initial) {
    if (group)
        suspendMessages();
    generation++;
    group = selected;
    browsing = false;
    reply = drafts.get(selected.id).reply;
    unseen = 0;
    newestSequence = Math.max(0, ...initial.map((message) => message.sequence));
    liveMessages.clear();
    initial.forEach((m) => liveMessages.set(m.sequence, m));
    messages.clear();
    initial.forEach((m) => messages.set(m.sequence, m));
    get("message-text").value =
        drafts.get(selected.id).text;
    sizeComposer();
    get("send-message").disabled = false;
    get("older-messages").disabled = false;
    updateReply();
    renderMessages();
    get("messages").scrollTop = get("messages").scrollHeight;
    updateComposer(selected);
}
export function updateComposer(selected) {
    group = selected;
    const input = get("message-text"), submit = get("send-message"), sending = drafts.isSending(selected.id);
    input.disabled = selected.archived;
    get("discuss-message").disabled = selected.archived || selected.kind === GroupKind.Board;
    input.readOnly = sending;
    submit.disabled = selected.archived || sending || !input.value.trim();
    submit.textContent = sending ? t("ux.sending") : t("html.edecf0ae6e");
    submit.setAttribute("aria-busy", String(sending));
    get("message-count").textContent = t("ux.messageCount", { count: input.value.length });
    get("composer-label").textContent = selected.archived
        ? t("messages.39338dbfe1")
        : selected.kind === GroupKind.Board
            ? t("messages.b921d5ef68")
            : t("messages.f32eb53703");
}
export function incoming(message) {
    if (message.group_id !== group?.id)
        return;
    liveMessages.set(message.sequence, message);
    trimMessages(liveMessages);
    if (message.sequence > newestSequence) {
        if (browsing || !atBottom())
            unseen++;
        newestSequence = message.sequence;
    }
    updateHistoryStatus();
    if (browsing)
        return;
    messages.set(message.sequence, message);
    trimMessages(messages);
    renderMessages();
}
function trimMessages(items) {
    while (items.size > 256)
        items.delete(Math.min(...items.keys()));
}
function atBottom() {
    const root = get("messages");
    return root.scrollHeight - root.scrollTop - root.clientHeight < 80;
}
function updateHistoryStatus() {
    get("history-status").textContent = browsing
        ? t("messages.f1fcde6deb")
        : t("messages.2bd1c987a4");
    const latest = get("latest-messages");
    latest.textContent = unseen ? t("messages.unseen", { count: unseen }) : t("html.1c59875d3a");
    latest.classList.toggle("has-unseen", unseen > 0);
}
function updateReply() {
    get("reply-status").textContent = reply
        ? t("messages.a40f9e2d59", { p0: reply })
        : "";
    get("cancel-reply").hidden = reply === null;
}
function renderMessages() {
    const root = get("messages"), bottom = atBottom();
    root.replaceChildren();
    updateHistoryStatus();
    for (const message of [...messages.values()].sort((a, b) => a.sequence - b.sequence)) {
        const contribution = message.event?.kind === GoalEventKind.Submitted && message.sender_id === message.event.person_id;
        const submission = resultForMessage(message, goalSnapshot());
        const author = submission?.work.person_id || message.sender_id;
        const row = el("article", undefined, "message" + (author === "operator" ? " own" : "") + (message.event && !contribution && !submission ? " system-event" : ""));
        const heading = el("header"), stamp = el("time", new Date(message.accepted_ms).toLocaleString("zh-CN"));
        stamp.dateTime = new Date(message.accepted_ms).toISOString();
        heading.append(el("strong", display(author)), stamp, button(t("messages.cf945e21dd"), () => {
            reply = message.sequence;
            updateReply();
            get("message-text").focus();
        }));
        if (message.reply_to)
            row.append(el("small", t("messages.a40f9e2d59", { p0: message.reply_to })));
        row.append(heading, el("p", message.event && !contribution
            ? t("GoalEvent." + message.event.kind, {
                title: message.event.title,
                person: display(message.event.person_id || "operator"),
                attempt: message.event.attempt,
            })
            : message.text));
        if (contribution) {
            row.classList.add("agent-contribution");
            row.append(button(t("messages.reviewResult"), () => focusGoal(message.event.goal_id)));
        }
        else if (submission) {
            const result = el("section", undefined, "message-result");
            result.append(el("strong", t("messages.resultBy", { person: display(submission.work.person_id) })), el("p", submission.result.code ? label("WorkResultCode", submission.result.code) : submission.result.summary), button(t("messages.reviewResult"), () => focusGoal(submission.goal.input.id)));
            row.append(result);
        }
        row.dataset.sequence = String(message.sequence);
        root.append(row);
    }
    if (!messages.size)
        root.append(el("p", group?.kind === GroupKind.Board
            ? t("messages.7bd4c97a5a")
            : t("messages.25b7624487"), "empty"));
    if (bottom)
        root.scrollTop = root.scrollHeight;
    document.dispatchEvent(new Event("workspace-messages"));
}
export function initMessages() {
    document.addEventListener("workspace-goals", renderMessages);
    get("messages").addEventListener("scroll", () => {
        if (!browsing && atBottom() && unseen) {
            unseen = 0;
            updateHistoryStatus();
        }
    });
    get("message-text").addEventListener("input", sizeComposer);
    const input = get("message-text");
    input.addEventListener("input", () => { if (group)
        updateComposer(group); });
    input.addEventListener("keydown", (event) => {
        if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && !event.isComposing) {
            event.preventDefault();
            if (!get("send-message").disabled)
                get("composer").requestSubmit();
        }
    });
    get("cancel-reply").onclick = () => {
        reply = null;
        updateReply();
    };
    get("older-messages").onclick = async () => {
        if (!group || !messages.size)
            return;
        const request = generation, history = ++historyGeneration, id = group.id;
        try {
            const page = await api("/api/crystal/view?" +
                query({ group_id: id, before: Math.min(...messages.keys()) }));
            if (request !== generation || history !== historyGeneration)
                return;
            if (!page.messages.length) {
                get("older-messages").disabled = true;
                return;
            }
            browsing = true;
            messages.clear();
            page.messages.forEach((m) => messages.set(m.sequence, m));
            renderMessages();
            get("messages").scrollTop = 0;
        }
        catch (error) {
            if (request === generation && history === historyGeneration)
                notice(error);
        }
    };
    get("latest-messages").onclick = async () => {
        if (!group)
            return;
        const request = generation, history = ++historyGeneration;
        try {
            const page = await api("/api/crystal/view?" + query({ group_id: group.id }));
            if (request !== generation || history !== historyGeneration)
                return;
            browsing = false;
            unseen = 0;
            messages.clear();
            page.messages.forEach((m) => messages.set(m.sequence, m));
            liveMessages.forEach((m) => messages.set(m.sequence, m));
            trimMessages(messages);
            newestSequence = Math.max(newestSequence, ...messages.keys());
            get("older-messages").disabled = false;
            renderMessages();
            get("messages").scrollTop = get("messages").scrollHeight;
        }
        catch (error) {
            if (request === generation && history === historyGeneration)
                notice(error);
        }
    };
    get("composer").onsubmit = async (event) => {
        event.preventDefault();
        if (!group || group.archived)
            return;
        const input = get("message-text"), text = input.value.trim(), id = group.id;
        const pending = drafts.begin(id, { text: input.value, reply });
        if (!pending)
            return;
        updateComposer(group);
        try {
            const result = await change({
                operation: "send",
                message: pending,
            });
            drafts.complete(pending);
            if (group?.id !== id)
                return;
            if (input.value.trim() === text && reply === pending.reply_to) {
                input.value = "";
                reply = null;
                sizeComposer();
                updateReply();
            }
            incoming(result.message);
            notice(t("messages.60823aaec7"));
        }
        catch (error) {
            if (group?.id === id)
                notice(error);
        }
        finally {
            drafts.finish(id);
            if (group?.id === id) {
                if (group)
                    updateComposer(group);
                input.focus();
            }
        }
    };
}
