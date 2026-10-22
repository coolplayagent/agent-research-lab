import { AdapterTestOperationState as State, CodingAgent } from "/assets/shared/contracts.js";
import { uuid } from "/assets/shared/identity.js";
import { api, ApiError } from "./api.js";
import { get, actions, button } from "./dom.js";
import { element, dialog, field, input } from "./forms.js";
import { t, label } from "./i18n.js";
import { editAdapter } from "./adapter-forms.js";
import { hasUnsavedSettings, savedSettings, trackSettings } from "./settings-guard.js";
import { adapterTestEndpoint, showAdapterProgress } from "./adapter-progress.js";
import type { Adapter, Configuration } from "./service-types.js";
import type { AdapterTestOperation, AdapterTestRequest } from "./adapter-test-types.js";
let generation = 0;
export function renderAdapters(config: Configuration, save: (value: Configuration) => Promise<void>, refresh: () => Promise<void>): void {
  const render = ++generation, container = get("adapter-list");
  container.replaceChildren();
  const cards = element("div"), status = element("p"), progress = element("div");
  status.setAttribute("role", "status");
  progress.setAttribute("aria-live", "polite"); progress.className = "binding-progress-list";
  const heading = element("h3", t("adapterTest.logs")); heading.hidden = true;
  container.append(cards, status, heading, progress);
  const controls = new Map<string, { test: HTMLButtonElement; edit: HTMLButtonElement; remove: HTMLButtonElement; state: HTMLElement }>();
  let records: AdapterTestOperation[] = [], pending: AdapterTestRequest | undefined;
  let posting = false, polling = false;
  const alive = () => render === generation && container.isConnected;
  const update = () => {
    for (const [id, row] of controls) {
      const operation = records.find(record => record.request.adapter_id === id);
      const running = operation?.state === State.Running;
      row.test.disabled = posting || running; row.edit.disabled = running; row.remove.disabled = running;
      const evidence = config.adapter_test_evidence?.[id];
      row.state.textContent = operation && operation.state !== State.Succeeded
        ? label("AdapterTestOperationState", operation.state)
        : evidence ? t("adapterTest.verified") : operation?.state === State.Succeeded ? t("adapterTest.changed") : t("adapterTest.untested");
    }
    heading.hidden = !records.length;
    showAdapterProgress(progress, records);
  };
  const poll = async () => {
    if (polling || !alive()) return;
    polling = true;
    try {
      const next = await api<AdapterTestOperation[]>(adapterTestEndpoint);
      if (!alive()) return;
      records = next.filter(record => config.adapters.some(a => a.id === record.request.adapter_id));
      update(); status.textContent = "";
      if (records.some(record => record.state === State.Succeeded && (record.configuration_revision ?? 0) > config.revision) && !hasUnsavedSettings()) {
        pending = undefined; document.dispatchEvent(new Event("workspace-services")); await refresh(); return;
      }
      const own = pending && records.find(record => record.request.request_id === pending!.request_id);
      if (own && own.state !== State.Running) pending = undefined;
    } catch { if (alive()) status.textContent = t("adapterTest.reconnecting"); }
    finally {
      polling = false;
      if (alive() && (pending || records.some(record => record.state === State.Running))) window.setTimeout(() => void poll(), 1000);
    }
  };
  const test = (adapter: Adapter) => {
    if (!alive() || posting || records.some(r => r.request.adapter_id === adapter.id && r.state === State.Running)) return;
    const box = dialog(t("adapterTest.dialog", { adapter: adapter.id }));
    const program = input(""), model = input("");
    program.placeholder = t("binding.programPlaceholder"); model.placeholder = t("binding.modelPlaceholder");
    if (adapter.agent !== CodingAgent.Custom) box.form.append(field(t("binding.program"), program), field(t("binding.model"), model), element("p", t("adapterTest.selection")));
    box.form.append(element("p", t("adapterTest.cost")));
    const submit = element("button", t("adapterTest.start")); submit.type = "submit"; submit.className = "primary";
    box.form.append(box.status, actions(submit)); trackSettings(box.form);
    box.form.onsubmit = async event => {
      event.preventDefault(); if (posting || submit.disabled) return;
      const candidate = { adapter_id: adapter.id, revision: config.revision, program: program.value.trim() || null, model: model.value.trim() || null };
      if (!pending || JSON.stringify(candidate) !== JSON.stringify({ adapter_id: pending.adapter_id, revision: pending.revision, program: pending.program, model: pending.model })) pending = { request_id: uuid(), ...candidate };
      posting = true; submit.disabled = true; update(); box.status.textContent = t("adapterTest.starting");
      try {
        const record = await api<AdapterTestOperation>(adapterTestEndpoint, pending, "manage-services");
        if (!alive()) return;
        records = records.filter(r => r.request.adapter_id !== adapter.id).concat(record);
        savedSettings(box.form); box.dialog.close(); update(); progress.scrollIntoView({ block: "nearest" });
      } catch (error) {
        if (error instanceof ApiError) pending = undefined;
        box.status.textContent = t("adapterTest.startError", { error: String(error) });
      } finally { posting = false; submit.disabled = false; if (alive()) { update(); void poll(); } }
    };
    box.dialog.showModal();
  };
  for (const adapter of config.adapters) {
    const card = element("article"); card.className = "card adapter-card";
    card.append(element("h3", `${adapter.id} · ${label("CodingAgent", adapter.agent)}`),
      element("p", adapter.enabled ? t("services.adapterEnabled") : t("services.adapterDisabled")),
      element("p", `${label("AgentProtocol", adapter.protocol)} · ${adapter.executor_id}`));
    const state = element("p"); state.className = "adapter-test-state"; card.append(state);
    const edit = button(t("services.edit"), () => editAdapter(config, adapter, save));
    const run = button(adapter.enabled ? t("adapterTest.retest") : t("adapterTest.start"), () => test(adapter)); run.className = "primary";
    const remove = button(t("services.remove"), async () => {
      if (!confirm(t("ux.confirmRemove", { title: adapter.id }))) return;
      remove.disabled = true;
      try { const next = structuredClone(config); next.adapters = next.adapters.filter(a => a.id !== adapter.id); await save(next); }
      catch (error) { status.textContent = String(error); remove.disabled = false; }
    });
    controls.set(adapter.id, { test: run, edit, remove, state });
    card.append(actions(run, edit, remove)); cards.append(card);
  }
  if (!config.adapters.length) cards.append(element("p", t("layout.noAdapters")));
  update(); void poll();
}
