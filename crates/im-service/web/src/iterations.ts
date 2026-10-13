import { uuid } from "/assets/shared/identity.js";
import { GroupKind } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, notice } from "./dom.js";
import { iterationForm } from "./iteration-form.js";
import { iterationCard, iterationDetail } from "./iteration-render.js";
import { reviewIteration } from "./iteration-review.js";
import { scenarioDescriptor } from "./scenario-catalog.js";
import { t } from "./i18n.js";
import { revealResearch } from "./workspace.js";
import { refreshEvolution, focusEvolutionSubject } from "./evolution.js";
import type { Group } from "./types.js";
import type { IterationInput, IterationPage, IterationRun } from "./iteration-types.js";

let group: Group | null = null, generation = 0, request = 0, selected = "";
let runs: IterationRun[] = [], after: string | null = null;
let detailKey = "";
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
const requestKeys = new Map<string, string>();

export function selectIterations(value: Group | null): void {
  if (value?.id && value.id === group?.id) { group = value; updateCreate(); return; }
  generation++;
  request++;
  clearTimeout(refreshTimer);
  group = value;
  selected = "";
  detailKey = "";
  runs = [];
  after = null;
  requestKeys.clear();
  get("iteration-list").replaceChildren();
  get("iteration-detail").replaceChildren();
  get("iteration-detail").hidden = true;
  get("more-iterations").hidden = true;
  get("iteration-status").textContent = t(value ? "iterations.loading" : "workspace.selectGroup");
  updateCreate();
  if (value && !get("iterations-page").hidden) void loadIterations();
}

function updateCreate(): void {
  get<HTMLButtonElement>("new-iteration").disabled = !group || group.archived || group.kind === GroupKind.Board;
}

export function refreshIterations(): void {
  if (!group || get("iterations-page").hidden || get("workspace").classList.contains("auxiliary")) return;
  clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => void loadIterations(), 120);
}

export async function focusIteration(id: string): Promise<void> {
  const context = generation;
  selected = id;
  revealResearch("iterations");
  await loadIterations();
  while (context === generation && !runs.some(run => run.input.id === id) && after) {
    const cursor = after;
    await loadIterations(true);
    if (cursor === after) break;
  }
  if (context !== generation) return;
  renderDetail();
  if (runs.some(run => run.input.id === id)) get("iteration-detail").scrollIntoView({ block: "nearest" });
  else notice(t("iterations.notFound"));
}

export async function loadIterations(more = false): Promise<void> {
  const selectedGroup = group?.id, context = generation, currentRequest = ++request;
  if (!selectedGroup) { get("iteration-status").textContent = t("workspace.selectGroup"); return; }
  try {
    const target = more ? 0 : runs.length;
    let cursor = more ? after || "" : "";
    const loaded: IterationRun[] = [];
    do {
      const page = await api<IterationPage>("/api/im/iterations?" + query({ group_id: selectedGroup, after: cursor }));
      if (context !== generation || currentRequest !== request) return;
      if (page.runs.some(run => run.input.group_id !== selectedGroup))
        throw new Error(t("iterations.scopeMismatch"));
      loaded.push(...page.runs);
      if (page.next_after && (page.next_after === cursor || loaded.length >= 8192))
        throw new Error(t("iterations.pageLimit"));
      cursor = page.next_after || "";
    } while (!more && cursor && loaded.length < target);
    const previous = new Map(runs.map(run => [run.input.id, run]));
    const next = new Map((more ? runs : []).map(run => [run.input.id, run]));
    for (const run of loaded) {
      const old = previous.get(run.input.id);
      next.set(run.input.id, old && old.revision > run.revision ? old : run);
    }
    runs = [...next.values()];
    after = cursor || null;
    render();
  } catch (error) {
    if (context === generation && currentRequest === request) {
      get("iteration-status").textContent = String(error);
    }
  }
}

function render(): void {
  get("iteration-list").replaceChildren(...runs.map(run => iterationCard(run, () => {
    selected = run.input.id;
    renderDetail();
  })));
  get("iteration-status").textContent = runs.length
    ? t("iterations.count", { count: runs.length }) : t("iterations.empty");
  get("more-iterations").hidden = !after;
  renderDetail();
}

function renderDetail(): void {
  const run = runs.find(item => item.input.id === selected);
  const panel = get("iteration-detail");
  panel.hidden = !run;
  get("close-iteration").hidden = !run;
  if (!run) { detailKey = ""; panel.replaceChildren(); return; }
  const reviewEnabled = scenarioDescriptor(run.input.scenario_id)?.manual_review === true;
  const key = `${run.input.id}:${run.revision}:${reviewEnabled}`;
  if (key === detailKey) return;
  detailKey = key;
  panel.replaceChildren(...iterationDetail(run,
    () => control(run, "pause"),
    () => control(run, "resume"),
    async () => {
      if (confirm(t("iterations.confirmStop", { title: run.input.title }))) await control(run, "stop");
    },
    async () => {
      if (await focusEvolutionSubject(run.input.subject_id)) revealResearch("evolution");
    },
    reviewEnabled ? () => reviewIteration(run, action => controlAction(run, action)) : undefined,
  ));
}

function requestKey(key: string): string {
  let value = requestKeys.get(key);
  if (!value) {
    value = "iteration-request-" + uuid();
    if (requestKeys.size >= 256) requestKeys.delete(requestKeys.keys().next().value!);
    requestKeys.set(key, value);
  }
  return value;
}

async function control(run: IterationRun, operation: string): Promise<void> {
  await controlAction(run, { operation });
}

async function controlAction(run: IterationRun, action: Record<string, unknown>): Promise<void> {
  const context = generation;
  try {
    await api("/api/im/iterations", {
      request_id: requestKey(`${run.input.id}:${run.revision}:${JSON.stringify(action)}`),
      run_id: run.input.id, revision: run.revision, action,
    });
  } finally {
    if (context === generation) { await loadIterations(); refreshEvolution(); }
  }
}

async function create(input: IterationInput): Promise<void> {
  const context = generation;
  await api("/api/im/iterations", {
    request_id: requestKey(`create:${input.id}`), run_id: input.id, revision: 0,
    action: { operation: "create", input },
  });
  if (context === generation && input.group_id === group?.id) {
    selected = input.id;
    await loadIterations();
    refreshEvolution();
  }
}

export function initIterations(): void {
  get("refresh-iterations").onclick = () => void loadIterations();
  get("more-iterations").onclick = () => void loadIterations(true);
  get("new-iteration").onclick = () => {
    if (group && !group.archived && group.kind !== GroupKind.Board)
      void iterationForm(group, create).catch(notice);
  };
  get("close-iteration").onclick = () => { selected = ""; renderDetail(); };
}
