import { BindingOperationState, CodingAgent, Presence } from "/assets/shared/contracts.js";
import { uuid } from "/assets/shared/identity.js";
import { api, ApiError } from "./api.js";
import { get, actions, button } from "./dom.js";
import { element, field, input, select } from "./forms.js";
import { t, label } from "./i18n.js";
import { savedSettings, trackSettings } from "./settings-guard.js";
import { bindingEndpoint, showProgress } from "./binding-progress.js";
import { openPerson } from "./people.js";
let generation = 0;
let preferredAdapter = "";
export function chooseBinding(id) { preferredAdapter = id; location.hash = "#settings/bindings"; }
function adapterTitle(config, id) {
    const adapter = config.adapters.find(a => a.id === id);
    if (!adapter)
        return id;
    const name = label("CodingAgent", adapter.agent);
    return config.adapters.filter(a => a.agent === adapter.agent).length > 1 || adapter.agent === CodingAgent.Custom ? `${name} · ${id}` : name;
}
export function renderBindings(config, people, save, refresh, onDirectory) {
    const render = ++generation, container = get("binding-list");
    container.replaceChildren();
    if (!Object.keys(config.bindings).length)
        container.append(element("p", t("layout.noBindings")));
    const rowNames = new Map();
    let verifyBinding = async () => { };
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
        const name = element("span", `${people.find((p) => p.id === person)?.name || person} → ${adapterTitle(config, adapter)}`);
        const state = element("span", config.binding_evidence?.[person] ? t("binding.verified") : t("binding.unverified"));
        state.className = "binding-row-state";
        const verify = button(t("binding.reverify"), () => verifyBinding(person, adapter));
        verify.className = "primary";
        rowNames.set(person, { node: name, state, verify, adapter });
        const identity = element("div");
        identity.className = "binding-row-identity";
        identity.append(name, state);
        const controls = actions(verify, unbind);
        controls.className += " binding-row-actions";
        row.append(identity, controls);
        container.append(row);
    }
    const form = element("form");
    form.className = "settings-form binding-form";
    trackSettings(form);
    const person = select(people.filter((p) => p.id !== "operator").map((p) => [p.id, p.name]), people.find((p) => p.id !== "operator")?.id || "");
    const adapter = select(config.adapters.map((a) => [a.id, adapterTitle(config, a.id)]), config.adapters.some(a => a.id === preferredAdapter) ? preferredAdapter : config.adapters[0]?.id || "");
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
    status.className = "binding-request-status";
    const progress = element("div");
    progress.setAttribute("aria-live", "polite");
    form.append(element("h3", t("binding.formTitle")), field(t("services.person"), person), field(t("services.agent"), adapter), field(t("binding.program"), program), field(t("binding.model"), model), element("p", t("binding.cost")), actions(submit));
    progress.className = "binding-progress-list";
    container.append(form, status, progress);
    let pending;
    let records = [];
    let posting = false, polling = false, loadingPeople = false, directoryLoaded = false, directoryGeneration = 0;
    const directoryStatus = element("p"), hint = element("p");
    directoryStatus.setAttribute("role", "status");
    const update = () => {
        submit.disabled = posting || loadingPeople || !person.value || !adapter.value || records.some((r) => r.request.person_id === person.value && r.state === BindingOperationState.Running);
        const custom = config.adapters.find((a) => a.id === adapter.value)?.agent === CodingAgent.Custom;
        program.disabled = custom;
        model.disabled = custom;
        person.disabled = loadingPeople || !person.options.length;
        for (const [id, row] of rowNames) {
            const running = records.find(r => r.request.person_id === id && r.state === BindingOperationState.Running);
            row.verify.disabled = posting || !!running;
            row.state.textContent = running ? t("binding.running") : config.binding_evidence?.[id] ? t("binding.verified") : t("binding.unverified");
        }
        hint.replaceChildren();
        hint.hidden = loadingPeople || (!!person.value && !!adapter.value);
        if (!hint.hidden) {
            if (!person.value && directoryLoaded)
                hint.append(element("span", t("bindings.noPeople")));
            if (!adapter.value) {
                const link = element("a", t("workspace.adapters"));
                link.href = "#settings/adapters";
                hint.append(element("span", t("bindings.noAdapters")), link);
            }
        }
    };
    verifyBinding = async (personId, adapterId) => {
        if (posting || records.some(r => r.request.person_id === personId && r.state === BindingOperationState.Running))
            return;
        await startValidation(personId, adapterId, null, null);
        progress.scrollIntoView({ block: "nearest" });
    };
    person.onchange = update;
    adapter.onchange = update;
    update();
    const loadDirectory = async () => {
        const request = ++directoryGeneration;
        loadingPeople = true;
        directoryStatus.textContent = t("bindings.loading");
        update();
        try {
            const directory = [];
            let cursor = "";
            do {
                const page = await api("/api/v1/collaboration/people?after=" + encodeURIComponent(cursor));
                if (render !== generation || request !== directoryGeneration || !form.isConnected)
                    return;
                directory.push(...page.people);
                const next = page.next_after || "";
                if (next === cursor)
                    break;
                cursor = next;
            } while (cursor && directory.length < 10000);
            const selected = person.value;
            people = directory;
            directoryLoaded = true;
            const eligible = people.filter(p => p.id !== "operator");
            person.replaceChildren(...select(eligible.map(p => [p.id, p.name]), "").children);
            person.value = eligible.some(p => p.id === selected) ? selected : eligible[0]?.id || "";
            for (const [id, row] of rowNames)
                row.node.textContent = `${people.find(p => p.id === id)?.name || id} → ${adapterTitle(config, row.adapter)}`;
            onDirectory?.(people);
            directoryStatus.textContent = "";
        }
        catch (error) {
            if (render === generation && request === directoryGeneration && form.isConnected) {
                directoryLoaded = false;
                directoryStatus.textContent = t("bindings.loadFailed", { error: String(error) });
            }
        }
        finally {
            if (render === generation && request === directoryGeneration && form.isConnected) {
                loadingPeople = false;
                update();
            }
        }
    };
    const create = button(t("bindings.createPerson"), () => openPerson(null, identity => {
        if (render !== generation || !form.isConnected)
            return;
        people = [...people.filter(p => p.id !== identity.id), { ...identity, presence: Presence.Offline, connections: 0 }];
        person.replaceChildren(...select(people.filter(p => p.id !== "operator").map(p => [p.id, p.name]), "").children);
        person.value = identity.id;
        person.dispatchEvent(new Event("change", { bubbles: true }));
        void loadDirectory();
    }));
    const cleanup = button(t("bindings.cleanupPeople"), async () => {
        if (!confirm(t("bindings.confirmCleanup")))
            return;
        const result = await api("/api/v1/collaboration/people/cleanup", { revision: config.revision }, "manage-services");
        await loadDirectory();
        directoryStatus.textContent = t("bindings.cleanupDone", { count: result.archived });
        document.dispatchEvent(new Event("workspace-people"));
    });
    form.append(element("p", t("bindings.directoryNote")), actions(create, button(t("bindings.refreshPeople"), loadDirectory), cleanup), directoryStatus, hint);
    void loadDirectory();
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
    const startValidation = async (personId, adapterId, selectedProgram, selectedModel) => {
        const candidate = { person_id: personId, adapter_id: adapterId, revision: config.revision, program: selectedProgram, model: selectedModel };
        if (!pending || JSON.stringify(candidate) !== JSON.stringify({ person_id: pending.person_id, adapter_id: pending.adapter_id, revision: pending.revision, program: pending.program, model: pending.model }))
            pending = { request_id: uuid(), ...candidate };
        posting = true;
        status.textContent = t("binding.starting");
        update();
        try {
            const record = await api(bindingEndpoint, pending, "manage-services");
            records = records.filter((r) => r.request.person_id !== record.request.person_id).concat(record);
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
    form.onsubmit = async (event) => {
        event.preventDefault();
        if (submit.disabled)
            return;
        const custom = config.adapters.find((a) => a.id === adapter.value)?.agent === CodingAgent.Custom;
        await startValidation(person.value, adapter.value, custom ? null : program.value.trim() || null, custom ? null : model.value.trim() || null);
    };
    void poll();
}
