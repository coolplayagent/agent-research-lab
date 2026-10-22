import { AdapterTestOperationState, AdapterTestStepState } from "/assets/shared/contracts.js";
import type { AdapterTestOperation } from "./adapter-test-types.js";
import { api } from "./api.js";
import { element } from "./forms.js";
import { label, t } from "./i18n.js";
export const adapterTestEndpoint = "/api/v1/collaboration/adapter-tests";
export function showAdapterProgress(container: HTMLElement, records: AdapterTestOperation[]): void {
  container.replaceChildren();
  const latest = new Map<string, AdapterTestOperation>();
  for (const record of records.slice().sort((a, b) => b.started_unix_ms - a.started_unix_ms || b.request.request_id.localeCompare(a.request.request_id))) {
    if (!latest.has(record.request.adapter_id)) latest.set(record.request.adapter_id, record);
  }
  for (const record of latest.values()) {
    const card = element("article"); card.className = "card binding-progress adapter-test-progress";
    card.append(element("h3", t("adapterTest.logTitle", { adapter: record.request.adapter_id })), element("p", label("AdapterTestOperationState", record.state)));
    const steps = element("ol");
    let previous = 0;
    for (const step of record.steps) {
      const elapsed = step.state === AdapterTestStepState.Running ? Math.max(0, Date.now() - record.started_unix_ms - previous) : step.elapsed_ms;
      previous += step.elapsed_ms;
      steps.append(element("li", t("binding.step", { stage: label("AdapterTestStage", step.stage), state: label("AdapterTestStepState", step.state), seconds: (elapsed / 1000).toFixed(1) })));
    }
    steps.className = "binding-log-steps";
    card.append(steps);
    if (record.program) card.append(element("p", t("binding.found", { agent: label("CodingAgent", record.program.agent), version: record.program.version })), element("code", record.program.executable));
    if (record.diagnostic) {
      const error = record.diagnostic;
      card.append(element("p", label("AdapterTestErrorCode", error.code)), element("p", t("binding.diagnostic", { id: error.diagnostic_id, stage: label("AdapterTestStage", error.stage) })));
      if (error.os_code !== null) card.append(element("p", t("binding.osCode", { code: error.os_code })));
    }
    if (record.state === AdapterTestOperationState.Running) {
      const cancel = element("button", t("adapterTest.cancel")); cancel.type = "button";
      cancel.onclick = async () => {
        cancel.disabled = true;
        try { await api(adapterTestEndpoint + "/" + encodeURIComponent(record.request.request_id) + "/cancel", {}, "manage-services"); }
        catch (error) { card.append(element("p", String(error))); cancel.disabled = false; }
      };
      card.append(cancel);
    }
    container.append(card);
  }
}
