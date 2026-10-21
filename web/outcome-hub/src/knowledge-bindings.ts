import { OutcomeServiceKind, ServiceManagement } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { el, button, actions } from "./dom.js";
import { input, select, field, dialog, finish } from "./forms.js";
import { trackSettings } from "./settings-guard.js";
import type { Configuration } from "./service-types.js";
import type { KnowledgeConfiguration } from "./knowledge-settings.js";
type Save = (next: Configuration) => Promise<void>;
type Load = (service: string) => Promise<KnowledgeConfiguration>;

export function knowledgeBindings(config: Configuration) {
  return [
    ...Object.entries(config.knowledge_groups || {}).map(([id, binding]) => ({ scope: "group", id, binding })),
    ...Object.entries(config.knowledge_goals || {}).map(([id, binding]) => ({ scope: "goal", id, binding })),
    ...Object.entries(config.outcome_bindings || {}).flatMap(([id, entry]) => entry.knowledge ? [{ scope: "topic", id, binding: entry.knowledge }] : []),
  ];
}
export function renderKnowledgeBindings(panel: HTMLElement, config: Configuration, save: Save, load: Load): void {
  panel.replaceChildren(el("h3", t("knowledge.defaultsTitle")), el("p", t("knowledge.bindingNote")));
  const entries = knowledgeBindings(config);
  if (!entries.length) panel.append(el("p", t("knowledge.noBindings"), "knowledge-subtle"));
  for (const { scope, id, binding } of entries) {
    const row = el("div", undefined, "knowledge-binding-row"), info = el("div");
    info.append(el("span", t(scope === "topic" ? "research.topics" : scope === "goal" ? "knowledge.goal" : "knowledge.group"), "knowledge-badge"),
      el("strong", id), el("span", binding.repository), el("small", binding.service_id));
    row.append(info, actions(button(t("services.edit"), () => bindScope(config, save, load, scope, id)), button(t("services.unbind"), async () => {
      const next = structuredClone(config);
      if (scope === "topic") {
        const entry = next.outcome_bindings![id]; entry.knowledge = null;
        if (!Object.keys(entry.verifiers).length) delete next.outcome_bindings![id];
      } else delete (scope === "goal" ? next.knowledge_goals : next.knowledge_groups)?.[id];
      await save(next);
    })));
    panel.append(row);
  }
}
export function bindScope(config: Configuration, save: Save, load: Load, oldScope?: string, oldId?: string): void {
  const box = dialog(t("knowledge.binding")); box.dialog.className = "knowledge-dialog";
  const scope = select([["group", t("knowledge.group")], ["goal", t("knowledge.goal")], ["topic", t("research.topics")]], oldScope || "group");
  const id = input(oldId || ""), services = (config.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Knowledge && s.management === ServiceManagement.LocalManaged && s.enabled),
    service = select(services.map(s => [s.id, s.id]), services[0]?.id || ""), target = select([], ""), hint = el("p");
  scope.disabled = id.readOnly = !!oldId; id.required = service.required = target.required = true;
  let generation = 0, ready = false;
  const oldBinding = () => scope.value === "topic" ? config.outcome_bindings?.[id.value.trim()]?.knowledge
    : (scope.value === "goal" ? config.knowledge_goals : config.knowledge_groups)?.[id.value.trim()];
  const loadTargets = async () => {
    const request = ++generation, requestedService = service.value, preferred = oldBinding();
    ready = false; target.disabled = true; target.replaceChildren();
    const submit = box.form.querySelector<HTMLButtonElement>('button[type="submit"]');
    if (submit) submit.disabled = true;
    hint.textContent = t(requestedService ? "knowledge.loading" : "knowledge.noAvailableTargets");
    if (!requestedService) return;
    try {
      const current = await load(requestedService);
      if (request !== generation) return;
      const options = Object.entries(current.targets).filter(([, entry]) => entry.enabled).map(([name]) => [name, name] as [string, string]);
      const next = select(options, ""); target.replaceChildren(...Array.from(next.children));
      const old = preferred?.service_id === requestedService ? preferred.repository : undefined;
      const available = old && options.some(([name]) => name === old);
      // Existing unavailable bindings must be explicitly replaced, never silently reassigned.
      if (old && !available) {
        const missing = el("option", t("knowledge.unavailableTarget", { name: old })); missing.value = ""; missing.disabled = true;
        target.replaceChildren(missing, ...Array.from(target.children)); target.value = "";
      } else target.value = available ? old : options[0]?.[0] || "";
      ready = options.length > 0; target.disabled = !ready;
      if (submit) submit.disabled = !ready || !target.value;
      hint.textContent = t(old && !available ? "knowledge.replaceUnavailable" : ready ? "knowledge.selectTargetNote" : "knowledge.noAvailableTargets");
    } catch (error) {
      if (request === generation) hint.textContent = t("knowledge.loadFailed", { error: String(error) });
    }
  };
  let bindingKey = "";
  const update = () => {
    const old = oldBinding(), key = JSON.stringify(old || null);
    if (key === bindingKey) return;
    bindingKey = key; service.value = old?.service_id || services[0]?.id || "";
    return loadTargets();
  };
  scope.onchange = id.onchange = update; service.onchange = loadTargets;
  target.onchange = () => { const submit = box.form.querySelector<HTMLButtonElement>('button[type="submit"]'); if (submit) submit.disabled = !ready || !target.value; };
  const retry = button(t("knowledge.load"), loadTargets);
  const grid = el("div", undefined, "knowledge-form-grid");
  grid.append(field(t("knowledge.scope"), scope), field(t("knowledge.scopeId"), id), field(t("knowledge.service"), service), field(t("knowledge.selectTarget"), target));
  box.form.append(el("p", t("knowledge.bindingNote")), grid, hint, actions(retry));
  trackSettings(box.form);
  finish(box, async () => {
    if (!ready || !target.value || !id.value.trim()) throw new Error(t("knowledge.noAvailableTargets"));
    const next = structuredClone(config), binding = { service_id: service.value, repository: target.value };
    if (scope.value === "topic") {
      next.outcome_bindings ||= {};
      const entry = next.outcome_bindings[id.value.trim()] ||= { verifiers: {}, knowledge: null }; entry.knowledge = binding;
    } else {
      const key = scope.value === "goal" ? "knowledge_goals" : "knowledge_groups";
      next[key] ||= {}; next[key]![id.value.trim()] = binding;
    }
    await save(next);
  });
  void update();
}
