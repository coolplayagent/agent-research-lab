import { BindingOperationState, CodingAgent } from "/assets/shared/contracts.js";
import { uuid } from "/assets/shared/identity.js";
import { api, ApiError } from "./api.js";
import { get, actions } from "./dom.js";
import { element, field, input, select } from "./forms.js";
import { t } from "./i18n.js";
import { savedSettings, trackSettings } from "./settings-guard.js";
import { bindingEndpoint, showProgress } from "./binding-progress.js";
let generation = 0;
export function renderBindings(config, people, save, refresh) {
    const render = ++generation, container = get("binding-list");
    container.replaceChildren();
    if (!Object.keys(config.bindings).length)
        container.append(element("p", t("layout.noBindings")));
    for (const [person, adapter] of Object.entries(config.bindings)) {
        const row = element("div");
        row.className = "binding-row";
        const unbind = element("button", t("services.unbind"));
        unbind.type = "button";
        unbind.onclick = async () => {
            unbind.disabled = true;
            try {
                const next = structuredClone(config);
                delete next.bindings[person];
                await save(next);
            }
            catch (error) {
                status.textContent = String(error);
                unbind.disabled = false;
            }
        };
        row.append(element("span", `${people.find((p) => p.id === person)?.name || person} → ${adapter}`), element("span", config.binding_evidence?.[person] ? t("binding.verified") : t("binding.unverified")), unbind);
        container.append(row);
    }
    const form = element("form");
    form.className = "settings-form binding-form";
    trackSettings(form);
    const person = select(people.filter((p) => p.id !== "operator").map((p) => [p.id, p.name]), people.find((p) => p.id !== "operator")?.id || "");
    const adapter = select(config.adapters.map((a) => [a.id, a.id]), config.adapters[0]?.id || "");
    const program = input(""), model = input("");
    program.placeholder = t("binding.programPlaceholder");
    model.placeholder = t("binding.modelPlaceholder");
    person.required = true;
    adapter.required = true;
    const submit = element("button", t("binding.validate"));
    submit.type = "submit";
    submit.className = "primary";
    const status = element("p");
    status.setAttribute("role", "status");
    const progress = element("div");
    progress.setAttribute("aria-live", "polite");
    form.append(field(t("services.person"), person), field(t("services.adapter"), adapter), field(t("binding.program"), program), field(t("binding.model"), model), element("p", t("binding.cost")), actions(submit), status);
    container.append(form, progress);
    let pending;
    let records = [];
    let posting = false, polling = false;
    const update = () => {
        submit.disabled = posting || !person.value || !adapter.value || records.some((r) => r.request.person_id === person.value && r.state === BindingOperationState.Running);
        const custom = config.adapters.find((a) => a.id === adapter.value)?.agent === CodingAgent.Custom;
        program.disabled = custom;
        model.disabled = custom;
    };
    person.onchange = update;
    adapter.onchange = update;
    update();
    if (!person.value || !adapter.value)
        form.append(element("p", t("layout.bindingPrerequisite")));
    const poll = async () => {
        if (polling || render !== generation)
            return;
        polling = true;
        try {
            const next = await api(bindingEndpoint);
            if (render !== generation)
                return;
            records = next;
            showProgress(progress, records);
            update();
            const own = pending && records.find((r) => r.request.request_id === pending.request_id);
            if (own?.state === BindingOperationState.Succeeded || records.some((r) => r.state === BindingOperationState.Succeeded && (r.configuration_revision ?? 0) > config.revision)) {
                pending = undefined;
                document.dispatchEvent(new Event("workspace-services"));
                await refresh();
                return;
            }
            if (own && own.state !== BindingOperationState.Running)
                pending = undefined;
        }
        catch {
            if (render === generation)
                status.textContent = t("binding.reconnecting");
        }
        finally {
            polling = false;
            if (render === generation && container.isConnected && (pending || records.some((r) => r.state === BindingOperationState.Running)))
                window.setTimeout(() => void poll(), 1000);
        }
    };
    form.onsubmit = async (event) => {
        event.preventDefault();
        if (submit.disabled)
            return;
        const custom = config.adapters.find((a) => a.id === adapter.value)?.agent === CodingAgent.Custom;
        const candidate = { person_id: person.value, adapter_id: adapter.value, revision: config.revision, program: custom ? null : program.value.trim() || null, model: custom ? null : model.value.trim() || null };
        if (!pending || JSON.stringify(candidate) !== JSON.stringify({ person_id: pending.person_id, adapter_id: pending.adapter_id, revision: pending.revision, program: pending.program, model: pending.model }))
            pending = { request_id: uuid(), ...candidate };
        posting = true;
        status.textContent = t("binding.starting");
        update();
        try {
            const record = await api(bindingEndpoint, pending, "manage-services");
            records = records.filter((r) => r.request.request_id !== record.request.request_id).concat(record);
            savedSettings(form);
            status.textContent = "";
            showProgress(progress, records);
        }
        catch (error) {
            if (error instanceof ApiError)
                pending = undefined;
            status.textContent = t("binding.startError", { error: String(error) });
        }
        finally {
            posting = false;
            update();
            void poll();
        }
    };
    void poll();
}
