import { renderBindings } from "./binding-settings.js";
import { renderKnowledgeSettings } from "./knowledge-settings.js";
import { trackSettings, savedSettings, allowSettingsLeave, hasUnsavedSettings, } from "./settings-guard.js";
import { ServiceKind, ServiceManagement, ServiceHealth } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get, actions } from "./dom.js";
import { t, label } from "./i18n.js";
import { editService } from "./service-forms.js";
import { editAdapter } from "./adapter-forms.js";
import { renderExecutionStatus } from "./execution-status.js";
import { element, select, field } from "./forms.js";
import { busy } from "./feedback.js";
import { renderScenarios } from "./scenario-settings.js";
import { renderOutcomeSettings } from "./outcome-settings.js";
let current;
let directory = [];
let generation = 0;
function button(title, action) {
    const node = element("button", title);
    node.type = "button";
    node.onclick = async () => {
        const done = busy(node);
        if (!done)
            return;
        try {
            await action();
        }
        catch (error) {
            get("services-status").textContent = String(error);
        }
        finally {
            done();
        }
    };
    return node;
}
async function save(config) {
    await api("/api/v1/collaboration/services", config, "manage-services");
    document.dispatchEvent(new Event("workspace-services"));
    await loadServices(true);
}
function render(view) {
    const config = view.configuration;
    current = view;
    renderExecutionStatus(view, directory, () => loadServices(true));
    renderScenarios(view, save);
    renderOutcomeSettings(view, save);
    renderKnowledgeSettings(view, save);
    get("services-status").textContent = view.restart_required
        ? t("services.restart")
        : t("services.activeStorage", {
            p0: view.active_storage?.[0] || t("services.embedded"),
        });
    const list = get("service-list");
    list.replaceChildren();
    for (const service of config.services) {
        const health = view.health.find((h) => h.id === service.id), card = element("article"), status = health
            ? label("ServiceHealth", health.status)
            : t("services.unknown");
        card.className = "card";
        card.append(element("h3", label("ServiceKind", service.kind)), element("small", service.id), element("p", `${label("ServiceManagement", service.management || ServiceManagement.External)} · ${health?.status === ServiceHealth.Unavailable && health.started === false && service.management === ServiceManagement.LocalManaged ? t("settings.standby") : status}`), element("code", service.endpoint));
        const users = config.adapters.filter(a => a.executor_id === service.id);
        if (users.length)
            card.append(element("p", t("settings.usedBy", { agents: users.map(a => `${label("CodingAgent", a.agent)} (${a.id})`).join(" · ") })));
        if (service.sandbox_id)
            card.append(element("p", t("services.dependency", { p0: service.sandbox_id })));
        if (health?.info)
            card.append(element("p", health.info.capabilities.join(" · ")));
        if (health?.error && !(health.started === false && service.management === ServiceManagement.LocalManaged))
            card.append(element("p", health.error));
        card.append(actions(button(t("services.edit"), () => editService(config, service, save)), button(t("services.remove"), async () => {
            if (!confirm(t("ux.confirmRemove", { title: service.id })))
                return;
            const next = structuredClone(config);
            next.services = next.services.filter((s) => s.id !== service.id);
            await save(next);
        })));
        list.append(card);
    }
    if (!config.services.length)
        list.append(element("p", t("layout.noServices")));
    const adapters = get("adapter-list");
    adapters.replaceChildren();
    for (const adapter of config.adapters) {
        const card = element("article");
        card.className = "card";
        card.append(element("h3", `${adapter.id} · ${label("CodingAgent", adapter.agent)}`), element("p", adapter.enabled
            ? t("services.adapterEnabled")
            : t("services.adapterDisabled")), element("p", `${label("AgentProtocol", adapter.protocol)} · ${adapter.executor_id}`), actions(button(t("services.edit"), () => editAdapter(config, adapter, save)), button(t("services.remove"), async () => {
            if (!confirm(t("ux.confirmRemove", { title: adapter.id })))
                return;
            const next = structuredClone(config);
            next.adapters = next.adapters.filter((a) => a.id !== adapter.id);
            await save(next);
        })));
        adapters.append(card);
    }
    if (!config.adapters.length)
        adapters.append(element("p", t("layout.noAdapters")));
    const storage = get("storage-selection");
    storage.replaceChildren();
    const selected = select([
        ["", t("services.embedded")],
        ...config.services
            .filter((s) => s.kind === ServiceKind.Storage)
            .map((s) => [s.id, s.id]),
    ], config.storage_id || "");
    const storageForm = element("form");
    trackSettings(storageForm);
    storage.append(storageForm);
    storageForm.append(field(t("services.storage"), selected), element("p", t("services.storageNote")));
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
        }
        catch (error) {
            get("services-status").textContent = String(error);
        }
        finally {
            storeButton.disabled = false;
        }
    };
    renderBindings(config, directory, save, () => loadServices(true));
}
export async function loadServices(force = false) {
    if (!force && hasUnsavedSettings())
        return;
    const request = ++generation;
    get("services-status").textContent = t("services.checking");
    try {
        const view = await api("/api/v1/collaboration/services");
        const people = [];
        let cursor = "";
        do {
            const page = await api("/api/v1/collaboration/people?after=" + encodeURIComponent(cursor));
            if (request !== generation)
                return;
            people.push(...page.people);
            const next = page.next_after || "";
            if (next === cursor)
                break;
            cursor = next;
        } while (cursor && people.length < 10000);
        if (request !== generation || (!force && hasUnsavedSettings()))
            return;
        directory = people;
        render(view);
    }
    catch (error) {
        if (request === generation)
            get("services-status").textContent = String(error);
    }
}
export function initServices() {
    get("refresh-services").onclick = () => {
        if (allowSettingsLeave())
            void loadServices();
    };
    get("add-service").onclick = () => {
        if (current)
            editService(current.configuration, undefined, save);
    };
    get("add-adapter").onclick = () => {
        if (current)
            editAdapter(current.configuration, undefined, save);
    };
}
