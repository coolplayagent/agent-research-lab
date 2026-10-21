import { ServiceKind, ServiceManagement } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get } from "./dom.js";
import { element } from "./forms.js";
import { t, label } from "./i18n.js";
import type { ServiceView } from "./service-types.js";
import type { Person } from "./types.js";
export function renderExecutionStatus(view: ServiceView, people: Person[], refresh: () => Promise<void>): void {
  const root = get("execution-status"), config = view.configuration;
  root.replaceChildren(element("p", t("services.activeStorage", { p0: view.active_storage?.[0] || t("services.embedded") })));
  const used = new Set(Object.values(config.bindings).map(id => config.adapters.find(a => a.id === id)?.executor_id));
  for (const service of config.services.filter(s => s.kind === ServiceKind.Executor && (used.has(s.id) || view.health.some(h => h.id === s.id && h.started)))) {
    const health = view.health.find(h => h.id === service.id);
    root.append(element("p", `${label("ServiceKind", service.kind)} · ${health ? label("ServiceHealth", health.status) : t("services.unknown")}`));
  }
  for (const [person, id] of Object.entries(config.bindings)) {
    const adapter = config.adapters.find(a => a.id === id);
    root.append(element("p", `${people.find(p => p.id === person)?.name || person} · ${adapter ? label("CodingAgent", adapter.agent) : id} · ${config.binding_evidence?.[person] ? t("binding.verified") : t("binding.unverified")}`));
  }
  if (!Object.keys(config.bindings).length) root.append(element("p", t("settings.noExecutionYet")));
  if (config.services.some(s => s.enabled && s.kind === ServiceKind.Executor && s.management === ServiceManagement.LocalManaged)) {
    const prepare = element("button", t("settings.prepareRuntime")), status = element("p");
    prepare.type = "button"; status.setAttribute("role", "status");
    prepare.onclick = async () => {
      prepare.disabled = true; status.textContent = t("settings.preparingRuntime");
      try { await api("/api/v1/collaboration/services/prepare", { revision: config.revision }, "manage-services"); await refresh(); }
      catch (error) { status.textContent = String(error); prepare.disabled = false; }
    };
    root.append(prepare, element("p", t("settings.prepareNote")), status);
  }
  const advanced = element("a", t("settings.advanced")); advanced.href = "#settings/services"; root.append(advanced);
}
