import {
  trackSettings,
  savedSettings,
  allowSettingsLeave,
  hasUnsavedSettings,
} from "./settings-guard.js";
import type { Page, Person } from "./types.js";
import { ServiceKind } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get } from "./dom.js";
import { t, label } from "./i18n.js";
import { editAdapter, editService } from "./service-forms.js";
import { element, select, field } from "./forms.js";
import type { Configuration, ServiceView } from "./service-types.js";
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
    node.disabled = true;
    try {
      await action();
    } catch (error) {
      get("services-status").textContent = String(error);
    } finally {
      node.disabled = false;
    }
  };
  return node;
}
async function save(config: Configuration): Promise<void> {
  await api("/api/im/services", config, "manage-services");
  await loadServices(true);
}
function render(view: ServiceView): void {
  const config = view.configuration;
  current = view;
  get("services-status").textContent = view.restart_required
    ? t("services.restart")
    : t("services.activeStorage", {
        p0: view.active_storage?.[0] || t("services.embedded"),
      });
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
      element("h3", service.id),
      element("p", `${label("ServiceKind", service.kind)} · ${status}`),
      element("code", service.endpoint),
    );
    if (service.sandbox_id)
      card.append(
        element("p", t("services.dependency", { p0: service.sandbox_id })),
      );
    if (health?.info)
      card.append(element("p", health.info.capabilities.join(" · ")));
    if (health?.error) card.append(element("p", health.error));
    card.append(
      button(t("services.edit"), () => editService(config, service, save)),
      button(t("services.remove"), async () => {
        const next = structuredClone(config);
        next.services = next.services.filter((s) => s.id !== service.id);
        await save(next);
      }),
    );
    list.append(card);
  }
  const adapters = get("adapter-list");
  adapters.replaceChildren();
  for (const adapter of config.adapters) {
    const card = element("article");
    card.className = "card";
    card.append(
      element("h3", `${adapter.id} · ${label("CodingAgent", adapter.agent)}`),
      element(
        "p",
        adapter.enabled
          ? t("services.adapterEnabled")
          : t("services.adapterDisabled"),
      ),
      element(
        "p",
        `${label("AgentProtocol", adapter.protocol)} · ${adapter.executor_id}`,
      ),
      button(t("services.edit"), () => editAdapter(config, adapter, save)),
      button(t("services.remove"), async () => {
        const next = structuredClone(config);
        next.adapters = next.adapters.filter((a) => a.id !== adapter.id);
        await save(next);
      }),
    );
    adapters.append(card);
  }
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
  const storeButton = element("button", t("services.saveStorage"));
  storeButton.type = "submit";
  storageForm.append(storeButton);
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
  const bindings = get("binding-list");
  bindings.replaceChildren();
  for (const [person, id] of Object.entries(config.bindings)) {
    const row = element("div");
    row.className = "toolbar";
    row.append(
      element(
        "span",
        `${directory.find((p) => p.id === person)?.name || person} → ${id}`,
      ),
      button(t("services.unbind"), async () => {
        const next = structuredClone(config);
        delete next.bindings[person];
        await save(next);
      }),
    );
    bindings.append(row);
  }
  const form = element("form"),
    person = select(
      directory.filter((p) => p.id !== "operator").map((p) => [p.id, p.name]),
      directory.find((p) => p.id !== "operator")?.id || "",
    ),
    adapter = select(
      config.adapters.map((a) => [a.id, a.id]),
      config.adapters[0]?.id || "",
    );
  person.required = true;
  trackSettings(form);
  const submit = element("button", t("services.bind"));
  submit.type = "submit";
  submit.disabled = !person.value || !adapter.value;
  form.append(
    field(t("services.person"), person),
    field(t("services.adapter"), adapter),
    submit,
  );
  form.onsubmit = async (event) => {
    event.preventDefault();
    submit.disabled = true;
    try {
      const next = structuredClone(config);
      next.bindings[person.value.trim()] = adapter.value;
      await save(next);
      savedSettings(form);
    } catch (error) {
      get("services-status").textContent = String(error);
    } finally {
      submit.disabled = !person.value || !adapter.value;
    }
  };
  bindings.append(form);
}
export async function loadServices(force = false): Promise<void> {
  if (!force && hasUnsavedSettings()) return;
  const request = ++generation;
  get("services-status").textContent = t("services.checking");
  try {
    const view = await api<ServiceView>("/api/im/services");
    const people: Person[] = [];
    let cursor = "";
    do {
      const page = await api<Page<Person>>(
        "/api/im/people?after=" + encodeURIComponent(cursor),
      );
      if (request !== generation) return;
      people.push(...page.people);
      const next = page.next_after || "";
      if (next === cursor) break;
      cursor = next;
    } while (cursor && people.length < 10000);
    if (request !== generation || (!force && hasUnsavedSettings())) return;
    directory = people;
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
