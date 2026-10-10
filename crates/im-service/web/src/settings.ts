import { ServiceKind } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get } from "./dom.js";
import { t, label } from "./i18n.js";
import {
  element,
  editAdapter,
  editService,
  select,
  field,
} from "./service-forms.js";
import type { Configuration, ServiceView } from "./service-types.js";
let current: ServiceView | undefined;
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
  await loadServices();
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
  storage.append(
    field(t("services.storage"), selected),
    element("p", t("services.storageNote")),
    button(t("services.saveStorage"), async () => {
      const next = structuredClone(config);
      next.storage_id = selected.value || null;
      await save(next);
    }),
  );
  const bindings = get("binding-list");
  bindings.replaceChildren();
  for (const [person, id] of Object.entries(config.bindings)) {
    const row = element("div");
    row.className = "toolbar";
    row.append(
      element("span", `${person} → ${id}`),
      button(t("services.unbind"), async () => {
        const next = structuredClone(config);
        delete next.bindings[person];
        await save(next);
      }),
    );
    bindings.append(row);
  }
  const form = element("form"),
    person = element("input"),
    adapter = select(
      config.adapters.map((a) => [a.id, a.id]),
      config.adapters[0]?.id || "",
    );
  person.required = true;
  person.placeholder = t("services.personPlaceholder");
  const submit = element("button", t("services.bind"));
  submit.type = "submit";
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
    } catch (error) {
      get("services-status").textContent = String(error);
    } finally {
      submit.disabled = false;
    }
  };
  bindings.append(form);
}
export async function loadServices(): Promise<void> {
  get("services-status").textContent = t("services.checking");
  try {
    render(await api<ServiceView>("/api/im/services"));
  } catch (error) {
    get("services-status").textContent = String(error);
  }
}
export function initServices(): void {
  get("refresh-services").onclick = () => void loadServices();
  get("add-service").onclick = () => {
    if (current) editService(current.configuration, undefined, save);
  };
  get("add-adapter").onclick = () => {
    if (current) editAdapter(current.configuration, undefined, save);
  };
}
