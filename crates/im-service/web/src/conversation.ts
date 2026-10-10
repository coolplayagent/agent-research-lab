import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, change, query } from "./api.js";
import { close, get, notice, open, value } from "./dom.js";
import { loadGroups, metrics, setSelected } from "./directory.js";
import { incoming, resetMessages, updateComposer } from "./messages.js";
import type { Group, GroupView, Message } from "./types.js";
import { selectGoals, loadGoals } from "./goals.js";
export let current: Group | null = null;
let generation = 0,
  stream: EventSource | null = null,
  converting = false,
  editSnapshot: Group | null = null;
export function renderHeader(group: Group): void {
  get("group-title").textContent = group.title;
  get("group-topic").textContent = group.topic;
  get("group-summary").textContent = t("conversation.77a11c13f3", {
    p0: group.member_count,
    p1:
      group.kind === GroupKind.Temporary
        ? t("conversation.3ceac95dcf")
        : group.kind === GroupKind.Board
          ? t("conversation.ca4773c5ff")
          : t("conversation.ff40f5a38d"),
    p2: group.archived ? t("conversation.3788d7bcf0") : "",
  });
  get("convert-group").hidden =
    group.kind !== GroupKind.Temporary || group.archived;
  get("pin-group").textContent = group.pinned
    ? t("conversation.c92179b74a")
    : t("conversation.173f88d28e");
  get("archive-group").textContent = group.archived
    ? t("conversation.cbe48cfa3a")
    : t("conversation.5292ab1a36");
  get<HTMLButtonElement>("new-goal").disabled = group.archived;
  updateComposer(group);
}
export async function select(group: Group): Promise<void> {
  const request = ++generation;
  stream?.close();
  stream = null;
  current = null;
  get("active-conversation").hidden = true;
  get("empty-conversation").hidden = false;
  try {
    const page = await api<GroupView>(
      "/api/crystal/view?" + query({ group_id: group.id }),
    );
    if (request !== generation) return;
    current = page.group;
    setSelected(group.id);
    resetMessages(current, page.messages);
    renderHeader(current);
    selectGoals(current);
    get("empty-conversation").hidden = true;
    get("active-conversation").hidden = false;
    stream = new EventSource(
      "/api/crystal/events?" +
        query({
          group_id: group.id,
          sequence: page.messages.at(-1)?.sequence || 0,
        }),
    );
    stream.addEventListener("message", (event) => {
      if (request === generation)
        incoming(JSON.parse((event as MessageEvent).data) as Message);
    });
    stream.addEventListener("status", (event) => {
      if (request !== generation) return;
      const status = JSON.parse((event as MessageEvent).data) as {
        group: Group;
        metrics: Record<string, unknown>;
      };
      const changed = current?.revision !== status.group.revision;
      current = status.group;
      renderHeader(current);
      metrics(status.metrics);
      void loadGoals();
      if (changed) void loadGroups();
    });
    stream.onerror = () => {
      if (request === generation)
        get("connection").textContent = t("conversation.dd2b0733b6");
    };
    stream.onopen = () => {
      if (request === generation)
        get("connection").textContent = t("conversation.021bd7f674");
    };
    stream.addEventListener("fault", () =>
      notice(t("conversation.a94f45360c")),
    );
  } catch (error) {
    if (request === generation) notice(error);
  }
}
export async function refreshCurrent(): Promise<void> {
  if (!current) return;
  const id = current.id,
    page = await api<GroupView>("/api/crystal/view?" + query({ group_id: id }));
  if (current?.id !== id) return;
  current = page.group;
  renderHeader(current);
  await loadGroups();
}
export function initConversation(): void {
  for (const field of ["pinned", "archived"] as const)
    get(field === "pinned" ? "pin-group" : "archive-group").onclick =
      async () => {
        if (!current) return;
        try {
          const { id, revision, title, topic, archived, pinned } = current;
          await change({
            operation: "update",
            change: {
              id,
              revision,
              title,
              topic,
              archived,
              pinned,
              [field]: !current[field],
            },
          });
          await refreshCurrent();
        } catch (error) {
          notice(error);
        }
      };
  for (const convert of [false, true])
    get(convert ? "convert-group" : "edit-group").onclick = () => {
      if (!current) return;
      converting = convert;
      editSnapshot = { ...current };
      get("edit-title").textContent = convert
        ? t("conversation.e514f49ff0")
        : t("conversation.0eae0e9bbc");
      get<HTMLInputElement>("edit-name").value = current.title;
      get<HTMLTextAreaElement>("edit-topic").value = current.topic;
      get<HTMLTextAreaElement>("edit-topic").disabled = convert;
      notice("", "edit-status");
      get("edit-note").textContent = convert
        ? t("conversation.ac849d8ff0")
        : "";
      open("edit-dialog");
    };
  get<HTMLFormElement>("edit-form").onsubmit = async (event) => {
    event.preventDefault();
    if (!editSnapshot) return;
    try {
      const { id, revision, archived, pinned } = editSnapshot;
      if (converting)
        await change({
          operation: "convert",
          group_id: id,
          revision,
          title: value("edit-name"),
        });
      else
        await change({
          operation: "update",
          change: {
            id,
            revision,
            archived,
            pinned,
            title: value("edit-name"),
            topic: value("edit-topic"),
          },
        });
      close("edit-dialog");
      await refreshCurrent();
      if (converting) location.hash = "#groups";
    } catch (error) {
      notice(error, "edit-status");
    }
  };
}
