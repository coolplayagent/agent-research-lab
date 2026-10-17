import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { initMentions, insertMention, prepareDiscussion } from "./mentions.js";
import { messageRow } from "./message-view.js";
import { taskTimeline, progressCard } from "./task-progress.js";
import { api, change, query } from "./api.js";
import { el, get, notice } from "./dom.js";
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
        if (row.dataset.sequence === String(sequence)) {
            const details = row.closest("details");
            if (details)
                details.open = true;
            row.scrollIntoView({ block: "center" });
        }
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
    const fresh = message.sequence > newestSequence;
    liveMessages.set(message.sequence, message);
    trimMessages(liveMessages);
    if (message.sequence > newestSequence) {
        if (browsing || !atBottom())
            unseen++;
        newestSequence = message.sequence;
    }
    updateHistoryStatus();
    if (browsing) {
        if (fresh)
            document.dispatchEvent(new CustomEvent("workspace-message", { detail: message }));
        return;
    }
    messages.set(message.sequence, message);
    trimMessages(messages);
    renderMessages();
    if (fresh)
        document.dispatchEvent(new CustomEvent("workspace-message", { detail: message }));
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
    const opened = new Set([...root.querySelectorAll("details[open][data-progress-key]")].map(node => node.dataset.progressKey));
    root.replaceChildren();
    updateHistoryStatus();
    const goals = goalSnapshot();
    for (const item of taskTimeline([...messages.values()], goals)) {
        if ("progress" in item) {
            root.append(progressCard(item.progress, opened.has(item.progress.key), focusGoal));
            continue;
        }
        const { message } = item;
        root.append(messageRow(message, messages, goals, {
            mention: insertMention,
            locate: locateMessage,
            review: focusGoal,
            reply: () => {
                reply = message.sequence;
                const author = message.event?.person_id || message.sender_id;
                if (author !== "operator")
                    insertMention(author);
                updateReply();
                get("message-text").focus();
            },
        }));
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
    initMentions();
    document.addEventListener("workspace-goals", renderMessages);
    document.addEventListener("workspace-people", renderMessages);
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
            const page = await api("/api/v1/messaging/view?" +
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
            const page = await api("/api/v1/messaging/view?" + query({ group_id: group.id }));
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
            if (drafts.discussion(pending) === undefined)
                drafts.setDiscussion(pending, await prepareDiscussion(id, text));
            const goal = drafts.discussion(pending);
            const result = goal
                ? await api("/api/v1/collaboration/discussion", { message: pending, goal })
                : await change({ operation: "send", message: pending });
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
