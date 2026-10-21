import { OutcomeKind, OutcomeServiceKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { el, button, actions } from "./dom.js";
import { input, check, select, area, field, dialog, finish } from "./forms.js";
import type { Configuration, ServiceView } from "./service-types.js";
import type { OutcomeService } from "./outcome-types.js";
type Save = (config: Configuration) => Promise<void>;

export function renderOutcomeServices(view: ServiceView, panel: HTMLElement, kind: OutcomeServiceKind, save: Save): void {
  const config = view.configuration;
  panel.replaceChildren(el("h2", t(kind === OutcomeServiceKind.Knowledge ? "knowledge.services" : "outcome.services")),
    el("p", t(kind === OutcomeServiceKind.Knowledge ? "knowledge.servicesNote" : "outcome.servicesNote")),
    button(t(kind === OutcomeServiceKind.Knowledge ? "knowledge.addService" : "outcome.addService"), () => editOutcomeService(config, undefined, save, kind)));
  for (const service of config.outcome_services || []) {
    if (service.kind !== kind) continue;
    const card = el("article", undefined, "card"), health = view.outcome_health?.find(h => h.id === service.id);
    card.append(el("h3", service.id), el("p", `${label("OutcomeServiceKind", service.kind)} · ${t(service.enabled ? "knowledge.enabled" : "knowledge.disabled")} · ${health ? label("ServiceHealth", health.status) : t("services.unknown")}`),
      el("code", service.endpoint), el("p", service.outcome_kinds.map(k => label("OutcomeKind", k)).join(" · ")),
      actions(button(t("services.edit"), () => editOutcomeService(config, service, save, kind)), button(t("services.remove"), async () => {
        if (!confirm(t("ux.confirmRemove", { title: service.id }))) return;
        const next = structuredClone(config); next.outcome_services = next.outcome_services?.filter(s => s.id !== service.id); await save(next);
      })));
    panel.append(card);
  }
}

export function editOutcomeService(config: Configuration, old: OutcomeService | undefined, save: Save, serviceKind: OutcomeServiceKind): void {
  const box = dialog(t(serviceKind === OutcomeServiceKind.Knowledge ? "knowledge.addService" : "outcome.addService")), id = input(old?.id || ""), endpoint = input(old?.endpoint || ""), enabled = check(old?.enabled ?? true),
    kind = select([[serviceKind, label("OutcomeServiceKind", serviceKind)]], serviceKind), seconds = input(String(old?.max_seconds || 120), "number"), dependencies = area(old?.dependencies.join("\n") || "");
  kind.disabled = true;
  id.required = endpoint.required = true; id.readOnly = !!old; seconds.min = "1"; seconds.max = "600";
  box.form.append(field(t("research.id"), id), field(t("outcome.serviceKind"), kind), field(t("outcome.endpoint"), endpoint), field(t("outcome.enabled"), enabled), field(t("outcome.seconds"), seconds), field(t("outcome.dependencies"), dependencies));
  const kinds = Object.values(OutcomeKind).map(kind => ({ kind, node: check(old?.outcome_kinds.includes(kind) ?? true) }));
  for (const { kind, node } of kinds) box.form.append(field(label("OutcomeKind", kind), node));
  finish(box, async () => {
    if (!old && config.outcome_services?.some(service => service.id === id.value.trim())) throw new Error(t("services.duplicateId"));
    const next = structuredClone(config); const entry: OutcomeService = { id: id.value.trim(), endpoint: endpoint.value.trim(), kind: kind.value as OutcomeServiceKind, management: old?.management, enabled: enabled.checked, protocol_version: 1, outcome_kinds: kinds.filter(k => k.node.checked).map(k => k.kind), dependencies: dependencies.value.split("\n").map(v => v.trim()).filter(Boolean), max_seconds: Number(seconds.value) }; next.outcome_services = [...(next.outcome_services || []).filter(s => s.id !== entry.id), entry]; await save(next); });
}
