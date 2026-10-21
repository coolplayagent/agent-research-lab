import { KnowledgeBackend, KnowledgeAccess, OutcomeServiceKind, ServiceManagement } from "/assets/shared/contracts.js";
import { knowledgeForm } from "./knowledge-form.js";
import { knowledgeCard } from "./knowledge-card.js";
import { knowledgeBindings, renderKnowledgeBindings, bindScope } from "./knowledge-bindings.js";
import { api } from "./api.js";
import { t } from "./i18n.js";
import { el, button, get, actions } from "./dom.js";
import type { Configuration, ServiceView } from "./service-types.js";
export interface KnowledgeTarget {
  kind: KnowledgeBackend; path: string; content_root: string; enabled: boolean;
  access: KnowledgeAccess; reference: string | null; remote: string | null; credential_env: string[];
}
export interface KnowledgeConfiguration { revision: number; targets: Record<string, KnowledgeTarget> }
export function knowledgeRequest<T>(config: Configuration, service: string, request: unknown): Promise<T> {
  return api<T>("/api/v1/knowledge", { service_id: service, configuration_revision: config.revision, request }, "manage-knowledge", 75000);
}
export function renderKnowledgeSettings(view: ServiceView, saveBindings: (next: Configuration) => Promise<void>, refresh: () => Promise<void> = async () => {}): void {
  const panel = get("knowledge-targets"), config = view.configuration;
  const loadTargets = (service: string) => knowledgeRequest<KnowledgeConfiguration>(config, service, { operation: "list" });
  const local = async <T>(request: unknown): Promise<T> => {
    const latest = await api<ServiceView>("/api/v1/collaboration/services");
    try {
      const response = await api<{ result: T }>("/api/v1/knowledge/local", { configuration_revision: latest.configuration.revision, request }, "manage-knowledge", 75000);
      return response.result;
    } finally { await refresh(); }
  };
  const add = button(t("knowledge.add"), () => knowledgeForm(KnowledgeBackend.Git,
    async (name, target) => { await local({ operation: "add", name, target }); }, target => local({ operation: "inspect", target })));
  add.className = "primary";
  const binding = button(t("knowledge.binding"), () => bindScope(config, saveBindings, loadTargets));
  const header = el("header", undefined, "knowledge-header"), intro = el("div");
  intro.append(el("h2", t("knowledge.title")), el("p", t("knowledge.note")));
  header.append(intro, actions(binding, add));
  const defaults = el("section", undefined, "knowledge-defaults");
  renderKnowledgeBindings(defaults, config, saveBindings, loadTargets);
  const list = el("section", undefined, "knowledge-services-list"), available = (config.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Knowledge && s.management === ServiceManagement.LocalManaged && s.enabled);
  const loaded = new Map<string, boolean>();
  binding.disabled = true; binding.title = t("knowledge.bindingPrerequisite");
  panel.replaceChildren(header, list, defaults);
  if (!available.length) {
    const empty = el("div", undefined, "knowledge-empty");
    empty.append(el("h3", t("knowledge.emptyTitle")), el("p", t("knowledge.empty"))); list.append(empty);
  }
  for (const service of available) {
    const section = el("section", undefined, "knowledge-service-section"), status = el("p", undefined, "knowledge-load-status"), targets = el("div", undefined, "cards knowledge-grid");
    status.setAttribute("role", "status");
    let generation = 0;
    const load = async () => {
      const request = ++generation;
      loaded.delete(service.id); binding.disabled = ![...loaded.values()].some(Boolean);
      status.textContent = t("knowledge.loading"); targets.replaceChildren();
      try {
        const current = await loadTargets(service.id);
        if (request !== generation || !section.isConnected) return;
        status.textContent = "";
        loaded.set(service.id, Object.values(current.targets).some(target => target.enabled));
        binding.disabled = ![...loaded.values()].some(Boolean);
        binding.title = binding.disabled ? t("knowledge.bindingPrerequisite") : "";
        const inspect = (target: KnowledgeTarget) => knowledgeRequest<{ target: KnowledgeTarget; revision: string }>(config, service.id, { operation: "inspect", target });
        const save = async (next: KnowledgeConfiguration) => { await knowledgeRequest(config, service.id, { operation: "save", configuration: next }); await load(); };
        const edit = (name?: string) => knowledgeForm(name ? current.targets[name].kind : KnowledgeBackend.Git, async (id, target) => {
          if (!name && Object.hasOwn(current.targets, id)) throw new Error(t("knowledge.duplicateId"));
          const prepared = await inspect(target), next = structuredClone(current); next.targets[id] = prepared.target; await save(next);
        }, inspect, name, name ? current.targets[name] : undefined);
        if (!Object.keys(current.targets).length) targets.append(el("p", t("knowledge.empty"), "knowledge-empty"));
        for (const [name, target] of Object.entries(current.targets)) {
          targets.append(knowledgeCard(name, target, {
            edit: () => edit(name),
            probe: () => knowledgeRequest(config, service.id, { operation: "probe", target: name }),
            toggle: async () => { const next = structuredClone(current); next.targets[name].enabled = !target.enabled; await save(next); },
            remove: async () => {
              const references = knowledgeBindings(config).filter(entry => entry.binding.service_id === service.id && entry.binding.repository === name);
              if (references.length) throw new Error(t("knowledge.removeBound", { name, count: references.length }));
              if (!confirm(t("knowledge.confirmRemove", { name }))) return;
              const next = structuredClone(current); delete next.targets[name]; await save(next);
            },
          }));
        }
      } catch (error) {
        if (request === generation && section.isConnected) status.textContent = t("knowledge.loadFailed", { error: String(error) });
      }
    };
    const serviceHeader = el("header", undefined, "knowledge-service-heading"), title = el("div");
    title.append(el("h3", t("knowledge.localTargets")));
    serviceHeader.append(title, button(t("knowledge.load"), load));
    section.append(serviceHeader, status, targets); list.append(section);
    void load();
  }
}
