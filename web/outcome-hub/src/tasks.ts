import { uuid } from "/assets/shared/identity.js";
import { GroupKind, TaskKind, TaskState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, el, button, actions, notice } from "./dom.js";
import { area, input, select, field, dialog, finish } from "./forms.js";
import { t } from "./i18n.js";
import { current } from "./conversation.js";
import { taskCard, taskContent } from "./task-render.js";
import type { OutcomeTask, TaskPage } from "./task-types.js";
import type { Directory, Group } from "./types.js";
import type { ServiceView } from "./service-types.js";
let selected: OutcomeTask | null = null, selectedNode = "", generation = 0, filter = "", next: string | null = null, rows: OutcomeTask[] = [], loading = false;
const endpoint = "/api/v2/collaboration/tasks";
async function groups(): Promise<Group[]> { const all: Group[] = []; let after = ""; do { const page = await api<Directory>("/api/v1/messaging/view?" + query({ after })); all.push(...page.directory.groups.filter(g => g.kind !== GroupKind.Board && !g.archived)); after = page.directory.next_after || ""; } while (after); return all; }
export async function newTask(groupId = current?.id): Promise<void> {
  const choices = await groups(), box = dialog(t("tasks.new")), intent = area(""), scope = select(choices.map(g => [g.id, g.title]), groupId || choices[0]?.id || ""); intent.required = true; intent.maxLength = 8000;
  box.form.append(el("p", t("tasks.newNote")), field(t("tasks.intent"), intent), field(t("tasks.conversation"), scope));
  if (!choices.length) { box.form.append(el("p", t("tasks.needConversation"))); box.dialog.showModal(); return; }
  const id = "task-" + uuid();
  finish(box, async () => { const task = await api<OutcomeTask>(endpoint, { operation: "create", input: { id, group_id: scope.value, intent: intent.value.trim() } }); location.hash = "tasks/" + encodeURIComponent(task.input.id); });
}
async function change(action: object): Promise<void> { if (!selected) return; selected = await api<OutcomeTask>(endpoint, { operation: "change", id: selected.input.id, revision: selected.revision, action }); renderDetail(); }
function renderDetail(): void {
  if (!selected) return;
  const root = get("task-detail-content"), opened = new Set([...root.querySelectorAll("details[open]")].map(n => n.querySelector("summary")?.textContent));
  root.replaceChildren(taskContent(selected, change, id => { selectedNode = id; renderDetail(); root.querySelector(".task-node-detail")?.scrollIntoView({ block: "nearest" }); }, selectedNode));
  root.querySelectorAll("details").forEach(n => { n.open = opened.has(n.querySelector("summary")?.textContent); });
  get("task-followup").hidden = selected.kind === TaskKind.Question || selected.state === TaskState.Completed || selected.state === TaskState.Cancelled;
  get("task-followup-send").textContent = selected.state === TaskState.Review ? t("tasks.return") : t("tasks.supplement");
}
function renderList(): void {
  const root = get("task-list"); root.replaceChildren();
  for (const task of rows.filter(task => !filter || task.state === filter)) root.append(taskCard(task, () => { location.hash = "tasks/" + encodeURIComponent(task.input.id); }));
  if (!root.childElementCount) root.append(el("p", t("tasks.empty"), "task-empty"));
  get("more-tasks").hidden = !next;
}
export async function loadTasks(more = false): Promise<void> {
  const path = location.hash.slice(1); if (!path.startsWith("tasks") || loading) return;
  const request = ++generation; loading = true;
  try {
    const id = path.startsWith("tasks/") ? decodeURIComponent(path.slice(6)) : "";
    get("task-list-view").hidden = !!id; get("task-detail-view").hidden = !id;
    if (id) {
      const task = await api<OutcomeTask>(endpoint + "?" + query({ id })); if (request !== generation || location.hash.slice(1) !== path) return;
      if (selected?.input.id !== id) { selectedNode = ""; get<HTMLTextAreaElement>("task-note").value = ""; }
      if (selected?.input.id !== id || selected.revision !== task.revision) { selected = task; renderDetail(); }
    } else {
      selected = null; const page = await api<TaskPage>(endpoint + "?" + query({ after: more ? next || "" : "" })); if (request !== generation || location.hash.slice(1) !== path) return;
      rows = more ? [...rows, ...page.tasks] : page.tasks; next = page.next_after; renderList();
    }
    get("tasks-status").textContent = "";
  } catch (e) { get("tasks-status").textContent = String(e); } finally { loading = false; }
}
async function budget(): Promise<void> {
  if (!selected) return; const task = selected, box = dialog(t("tasks.budget")), values = { ...task.input.budget }, controls = new Map<string, HTMLInputElement>();
  for (const [key, value] of Object.entries(values)) { const node = input(String(value), "number"); node.min = "1"; controls.set(key, node); box.form.append(field(t("tasks.budget." + key), node)); }
  finish(box, async () => { if (selected?.input.id !== task.input.id) throw new Error(t("tasks.changed")); await change({ operation: "budget", budget: Object.fromEntries([...controls].map(([key, node]) => [key, Number(node.value)])) }); });
}
export async function loadCoordinator(): Promise<void> {
  const root = get("coordinator-settings");
  try {
    const services = await api<ServiceView>("/api/v1/collaboration/services");
    let config = services.configuration;
    { try { const value = await api<{ revision: number; coordinator: NonNullable<typeof config.coordinator> }>("/api/v2/collaboration/coordinator"); config = { ...config, revision: value.revision, coordinator: value.coordinator }; } catch { /* A new installation can configure its first adapter below. */ } }
    const adapter = select(config.adapters.filter(a => a.enabled).map(a => [a.id, a.id]), config.coordinator?.adapter_id || ""), model = input(config.coordinator?.model || "");
    root.replaceChildren(el("h2", t("tasks.coordinator")), el("p", t("tasks.coordinatorNote")), field(t("tasks.adapter"), adapter), field(t("tasks.model"), model), el("p", config.coordinator?.protocol_evidence ? t("tasks.coordinatorVerified") : t("tasks.coordinatorPending")), button(t("tasks.testCoordinator"), async () => { await api("/api/v2/collaboration/coordinator/test", {}, "manage-services", 95000); await loadCoordinator(); }), button(t("services.save"), async () => { await api("/api/v2/collaboration/coordinator", { revision: config.revision, adapter_id: adapter.value, model: model.value.trim() || null }, "manage-services"); await loadCoordinator(); }));
  } catch (e) { root.replaceChildren(el("p", String(e))); }
}
export function initTasks(): void {
  get("new-task").onclick = () => { void newTask().catch(notice); };
  get("new-goal").onclick = () => { void newTask().catch(notice); };
  get("refresh-tasks").onclick = () => { void loadTasks(); };
  get("more-tasks").onclick = () => { void loadTasks(true); };
  get<HTMLSelectElement>("task-filter").onchange = e => { filter = (e.target as HTMLSelectElement).value; renderList(); };
  get("task-budget").onclick = () => { void budget().catch(notice); };
  get("task-followup-send").onclick = async () => { const node = get<HTMLTextAreaElement>("task-note"), text = node.value.trim(); if (!text) return; try { await change(selected?.state === TaskState.Review ? { operation: "return", reason: text } : { operation: "supplement", text }); node.value = ""; } catch (e) { notice(e); } };
  setInterval(() => { if (!document.hidden) void loadTasks(); }, 3000);
}
