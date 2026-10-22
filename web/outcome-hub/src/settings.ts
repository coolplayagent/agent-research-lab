import { renderBindings } from "./binding-settings.js";
import { renderKnowledgeSettings } from "./knowledge-settings.js";
import {
  trackSettings,
  savedSettings,
  allowSettingsLeave,
  hasUnsavedSettings,
} from "./settings-guard.js";
import type { Person } from "./types.js";
import { ServiceKind, ServiceManagement, ServiceHealth } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get, actions } from "./dom.js";
import { t, label } from "./i18n.js";
import { editService } from "./service-forms.js";
import { editAdapter } from "./adapter-forms.js";
import { renderAdapters } from "./adapter-settings.js";
import { element, select, field } from "./forms.js";
import type { Configuration, ServiceView } from "./service-types.js";
import { busy } from "./feedback.js";
let current: ServiceView | undefined;
let directory: Person[] = [];
let generation = 0;
function button(
  title: string,
  action: () => void | Promise<void>,
): HTMLButtonElement {
  const node = element("button", title);
  node.type = "button";
  node.onclick = async () => {
    const done = busy(node);
    if (!done) return;
    try {
      await action();
    } catch (error) {
      get("services-status").textContent = String(error);
    } finally {
      done();
    }
  };
  return node;
}
async function save(config: Configuration): Promise<void> {
  await api("/api/v1/collaboration/services", config, "manage-services");
  document.dispatchEvent(new Event("workspace-services"));
  await loadServices(true);
}
function render(view: ServiceView): void {
  const config = view.configuration;
  current = view;
  renderKnowledgeSettings(view, save, () => loadServices(true));
  get("services-status").textContent = view.restart_required
    ? t("services.restart")
    : t("runtime.settingsNote");
  const list = get("service-list");
  list.replaceChildren();
  for (const service of config.services) {
    const health = view.health.find((h) => h.id === service.id),
      card = element("article"),
      status = health
        ? label("ServiceHealth", health.status)
        : t("services.unknown");
    card.className = "card";
    card.append(
      element("h3", label("ServiceKind", service.kind)),
      element("small", service.id),
      element("p", `${label("ServiceManagement", service.management || ServiceManagement.External)} · ${health?.status === ServiceHealth.Unavailable && health.started === false && service.management === ServiceManagement.LocalManaged ? t("settings.standby") : status}`),
      element("code", service.endpoint),
    );
    const users = config.adapters.filter(a => a.executor_id === service.id);
    if (users.length) card.append(element("p", t("settings.usedBy", { agents: users.map(a => `${label("CodingAgent", a.agent)} (${a.id})`).join(" · ") })));
    if (service.sandbox_id)
      card.append(
        element("p", t("services.dependency", { p0: service.sandbox_id })),
      );
    if (health?.info)
      card.append(element("p", health.info.capabilities.join(" · ")));
    if (health?.error && !(health.started === false && service.management === ServiceManagement.LocalManaged)) card.append(element("p", health.error));
    card.append(actions(
      button(t("services.edit"), () => editService(config, service, save)),
      button(t("services.remove"), async () => {
        if (!confirm(t("ux.confirmRemove", { title: service.id }))) return;
        const next = structuredClone(config);
        next.services = next.services.filter((s) => s.id !== service.id);
        await save(next);
      }),
    ));
    list.append(card);
  }
  if (!config.services.length) list.append(element("p", t("layout.noServices")));
  renderAdapters(config, save, () => loadServices(true));
  const storage = get("storage-selection");
  storage.replaceChildren();
  const selected = select(
    [
      ["", t("services.embedded")],
      ...config.services
        .filter((s) => s.kind === ServiceKind.Storage)
        .map((s) => [s.id, s.id] as [string, string]),
    ],
    config.storage_id || "",
  );
  const storageForm = element("form");
  trackSettings(storageForm);
  storage.append(storageForm);
  storageForm.append(
    field(t("services.storage"), selected),
    element("p", t("services.storageNote")),
  );
  storageForm.className = "settings-form";
  const storeButton = element("button", t("services.saveStorage"));
  storeButton.type = "submit";
  storeButton.className = "primary";
  storageForm.append(actions(storeButton));
  storageForm.onsubmit = async (event) => {
    event.preventDefault();
    storeButton.disabled = true;
    try {
      const next = structuredClone(config);
      next.storage_id = selected.value || null;
      await save(next);
      savedSettings(storageForm);
    } catch (error) {
      get("services-status").textContent = String(error);
    } finally {
      storeButton.disabled = false;
    }
  };
  renderBindings(config, directory, save, () => loadServices(true), people => {
    directory = people;
  });
}
export async function loadServices(force = false): Promise<void> {
  if (!force && hasUnsavedSettings()) return;
  const request = ++generation;
  get("services-status").textContent = t("services.checking");
  try {
    const view = await api<ServiceView>("/api/v1/collaboration/services");
    if (request !== generation || (!force && hasUnsavedSettings())) return;
    render(view);
  } catch (error) {
    if (request === generation)
      get("services-status").textContent = String(error);
  }
}
export function initServices(): void {
  get("refresh-services").onclick = () => {
    if (allowSettingsLeave()) void loadServices();
  };
  get("add-service").onclick = () => {
    if (current) editService(current.configuration, undefined, save);
  };
  get("add-adapter").onclick = () => {
    if (current) editAdapter(current.configuration, undefined, save);
  };
}
