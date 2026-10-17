import { ScenarioCapability } from "/assets/shared/contracts.js";
import { actions, button, el, get } from "./dom.js";
import { check, dialog, field, finish, input } from "./forms.js";
import { label, t } from "./i18n.js";
import { trackSettings } from "./settings-guard.js";
import type { Configuration, ScenarioRegistration, ServiceView } from "./service-types.js";

export function renderScenarios(view: ServiceView, save: (config: Configuration) => Promise<void>): void {
  const config = view.configuration, list = get("scenario-list");
  list.replaceChildren();
  for (const scenario of config.scenarios || []) {
    const health = view.scenario_health?.find(item => item.id === scenario.id);
    const card = el("article", undefined, "card");
    card.append(
      el("h3", scenario.id),
      el("p", t(scenario.enabled ? "scenarioSettings.enabled" : "scenarioSettings.disabled")),
      el("p", t("scenarioSettings.health", {
        status: health ? label("ServiceHealth", health.status) : t("services.unknown"),
      })),
      el("code", scenario.endpoint),
      el("p", t("scenarioSettings.protocol", { version: scenario.protocol_version })),
      el("p", scenario.capabilities.map(value => label("ScenarioCapability", value)).join(" · ")),
      el("p", t("scenarioSettings.dependencies", { dependencies: scenario.dependencies.join(" · ") || t("services.none") })),
    );
    if (health?.error) card.append(el("p", health.error));
    card.append(actions(
      button(t("services.edit"), () => editScenario(config, scenario, save)),
      button(t("services.remove"), async () => {
        if (!confirm(t("ux.confirmRemove", { title: scenario.id }))) return;
        const next = structuredClone(config);
        next.scenarios = (next.scenarios || []).filter(item => item.id !== scenario.id);
        await save(next);
      }),
    ));
    list.append(card);
  }
  if (!config.scenarios?.length) list.append(el("p", t("scenarioSettings.empty")));
  get("add-scenario").onclick = () => editScenario(config, undefined, save);
}

function editScenario(config: Configuration, old: ScenarioRegistration | undefined, save: (config: Configuration) => Promise<void>): void {
  const box = dialog(t("scenarioSettings.edit"));
  const id = input(old?.id || ""), endpoint = input(old?.endpoint || ""), enabled = check(old?.enabled ?? false);
  id.required = true;
  id.readOnly = !!old;
  endpoint.required = true;
  endpoint.placeholder = "/absolute/private/scenario.sock";
  const capabilityFields = new Map(Object.values(ScenarioCapability).map(value => [value, check(old?.capabilities.includes(value) || false)]));
  const dependencyFields = new Map(config.services.map(service => [service.id, check(old?.dependencies.includes(service.id) || false)]));
  box.form.append(
    el("p", t("scenarioSettings.note")), field(t("services.id"), id),
    field(t("services.endpoint"), endpoint), field(t("services.enabled"), enabled),
    el("p", t("scenarioSettings.protocol", { version: 1 })), el("h3", t("scenarioSettings.capabilities")),
  );
  for (const [capability, control] of capabilityFields) box.form.append(field(label("ScenarioCapability", capability), control));
  box.form.append(el("h3", t("scenarioSettings.dependenciesTitle")));
  for (const [serviceId, control] of dependencyFields) box.form.append(field(serviceId, control));
  trackSettings(box.form);
  finish(box, async () => {
    const next = structuredClone(config);
    next.scenarios = (next.scenarios || []).filter(item => item.id !== old?.id);
    next.scenarios.push({
      id: id.value.trim(), endpoint: endpoint.value.trim(), enabled: enabled.checked,
      protocol_version: 1,
      capabilities: [...capabilityFields].filter(([, control]) => control.checked).map(([value]) => value),
      dependencies: [...dependencyFields].filter(([, control]) => control.checked).map(([value]) => value),
    });
    await save(next);
  });
}
