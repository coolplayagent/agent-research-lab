import { BindingOperationState, BindingStepState } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { element } from "./forms.js";
import { label, t } from "./i18n.js";
export const bindingEndpoint = "/api/v1/collaboration/bindings";
export function showProgress(container, records) {
    container.replaceChildren();
    for (const record of records.slice().sort((a, b) => b.started_unix_ms - a.started_unix_ms).slice(0, 10)) {
        const card = element("article");
        card.className = "card binding-progress";
        card.append(element("h3", t("binding.operation", { person: record.request.person_id, adapter: record.request.adapter_id })), element("p", label("BindingOperationState", record.state)));
        const steps = element("ol");
        let previous = 0;
        for (const step of record.steps) {
            const elapsed = step.state === BindingStepState.Running ? Math.max(0, Date.now() - record.started_unix_ms - previous) : step.elapsed_ms;
            previous += step.elapsed_ms;
            steps.append(element("li", t("binding.step", { stage: label("BindingStage", step.stage), state: label("BindingStepState", step.state), seconds: (elapsed / 1000).toFixed(1) })));
        }
        card.append(steps);
        if (record.program)
            card.append(element("p", t("binding.found", { agent: label("CodingAgent", record.program.agent), version: record.program.version })), element("code", record.program.executable));
        if (record.diagnostic) {
            const error = record.diagnostic;
            card.append(element("p", label("BindingErrorCode", error.code)), element("p", t("binding.diagnostic", { id: error.diagnostic_id, stage: label("BindingStage", error.stage) })));
            if (error.os_code !== null)
                card.append(element("p", t("binding.osCode", { code: error.os_code })));
        }
        if (record.state === BindingOperationState.Running) {
            const cancel = element("button", t("binding.cancel"));
            cancel.type = "button";
            cancel.onclick = async () => {
                cancel.disabled = true;
                try {
                    await api(bindingEndpoint + "/" + encodeURIComponent(record.request.request_id) + "/cancel", {}, "manage-services");
                }
                catch (error) {
                    card.append(element("p", String(error)));
                    cancel.disabled = false;
                }
            };
            card.append(cancel);
        }
        container.append(card);
    }
}
