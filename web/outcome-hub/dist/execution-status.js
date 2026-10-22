import { LocalRuntimeComponent, LocalRuntimeState } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { get, actions } from "./dom.js";
import { element } from "./forms.js";
import { t, label } from "./i18n.js";
export function renderExecutionStatus(view, people, refresh) {
    const root = get("execution-status"), config = view.configuration;
    root.replaceChildren(element("p", t("runtime.intro")));
    const cards = element("div");
    cards.className = "runtime-cards";
    for (const runtime of view.local_runtime || []) {
        const card = element("article");
        card.className = "card runtime-card";
        card.append(element("h3", label("LocalRuntimeComponent", runtime.component)), element("p", label("LocalRuntimeState", runtime.state)));
        const help = runtime.component === LocalRuntimeComponent.Storage ? "runtime.storageHelp" : runtime.component === LocalRuntimeComponent.Execution ? "runtime.executionHelp" : "runtime.knowledgeHelp";
        card.append(element("p", t(help)));
        if (runtime.state === LocalRuntimeState.External || runtime.state === LocalRuntimeState.Development)
            card.append(element("p", t("runtime.existing")));
        if (runtime.health)
            card.append(element("p", label("ServiceHealth", runtime.health)));
        if (runtime.error)
            card.append(element("p", label("LocalRuntimeErrorCode", runtime.error)));
        if (runtime.state === LocalRuntimeState.Missing) {
            const download = element("a", t("runtime.download"));
            download.href = "https://github.com/coolplayagent/agent-research-lab/releases";
            card.append(download);
        }
        if (runtime.can_prepare) {
            const prepare = element("button", t("runtime.prepare")), status = element("p");
            prepare.type = "button";
            prepare.className = "primary";
            status.setAttribute("role", "status");
            prepare.onclick = async () => {
                prepare.disabled = true;
                status.textContent = t("settings.preparingRuntime");
                try {
                    await api("/api/v1/collaboration/services/prepare", { revision: config.revision }, "manage-services", 45000);
                    await refresh();
                }
                catch (error) {
                    status.textContent = String(error);
                    prepare.disabled = false;
                }
            };
            card.append(actions(prepare), element("p", t("runtime.prepareNote")), status);
        }
        cards.append(card);
    }
    root.append(cards);
    const links = actions();
    for (const [key, route] of [["workspace.bindings", "bindings"], ["knowledge.title", "knowledge"]]) {
        const link = element("a", t(key));
        link.href = "#settings/" + route;
        links.append(link);
    }
    root.append(element("p", t("runtime.agentHelp")), links);
    for (const [person, id] of Object.entries(config.bindings)) {
        const adapter = config.adapters.find(a => a.id === id);
        root.append(element("p", `${people.find(p => p.id === person)?.name || person} · ${adapter ? label("CodingAgent", adapter.agent) : id} · ${config.binding_evidence?.[person] ? t("binding.verified") : t("binding.unverified")}`));
    }
    const advanced = element("a", t("settings.advanced"));
    advanced.href = "#settings/services";
    root.append(advanced);
}
