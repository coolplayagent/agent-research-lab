import { CodingAgent, AgentProtocol, ServiceKind, } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { element, select, input, check, area, dialog, finish, field, } from "./forms.js";
export function editService(config, old, save) {
    const box = dialog(t("services.editService")), id = input(old?.id || ""), kind = select(Object.values(ServiceKind).map((value) => [
        value,
        label("ServiceKind", value),
    ]), old?.kind || ServiceKind.Executor), endpoint = input(old?.endpoint || ""), enabled = check(old?.enabled ?? true), sandbox = select([
        ["", t("services.none")],
        ...config.services
            .filter((s) => s.kind === ServiceKind.Sandbox)
            .map((s) => [s.id, s.id]),
    ], old?.sandbox_id || "");
    id.required = true;
    id.readOnly = !!old;
    endpoint.required = true;
    endpoint.placeholder = "/absolute/private/service.sock";
    box.form.append(field(t("services.id"), id), field(t("services.kind"), kind), field(t("services.endpoint"), endpoint), field(t("services.enabled"), enabled), field(t("services.sandbox"), sandbox));
    finish(box, async () => {
        const value = {
            id: id.value.trim(),
            kind: kind.value,
            endpoint: endpoint.value.trim(),
            enabled: enabled.checked,
            sandbox_id: kind.value === ServiceKind.Executor ? sandbox.value || null : null,
        };
        const next = structuredClone(config);
        next.services = next.services.filter((s) => s.id !== old?.id);
        next.services.push(value);
        await save(next);
    });
}
export function editAdapter(config, old, save) {
    const box = dialog(t("services.editAdapter")), id = input(old?.id || ""), agent = select(Object.values(CodingAgent).map((value) => [
        value,
        label("CodingAgent", value),
    ]), old?.agent || CodingAgent.Custom), executor = select(config.services
        .filter((s) => s.kind === ServiceKind.Executor)
        .map((s) => [s.id, s.id]), old?.executor_id || "executor"), command = area(JSON.stringify(old?.command || [], null, 2)), env = area((old?.env_names || []).join("\n")), mounts = area((old?.read_only_paths || []).join("\n")), enabled = check(old?.enabled ?? false), network = check(old?.network ?? true), seconds = input(String(old?.max_seconds || 1800), "number");
    id.required = true;
    id.readOnly = !!old;
    seconds.min = "1";
    seconds.max = "3600";
    seconds.required = true;
    box.form.append(element("p", t("services.adapterNote")), field(t("services.id"), id), field(t("services.agent"), agent), field(t("services.executor"), executor), field(t("services.command"), command), element("p", t("services.commandNote")), field(t("services.env"), env), element("p", t("services.envNote")), field(t("services.mounts"), mounts), field(t("services.network"), network), field(t("services.limit"), seconds), field(t("services.enabled"), enabled));
    finish(box, async () => {
        const argv = JSON.parse(command.value);
        if (!Array.isArray(argv) || !argv.every((v) => typeof v === "string"))
            throw new Error(t("services.commandInvalid"));
        const lines = (node) => node.value
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean);
        const value = {
            id: id.value.trim(),
            agent: agent.value,
            protocol: AgentProtocol.JsonStdioV1,
            executor_id: executor.value,
            command: argv,
            env_names: lines(env),
            read_only_paths: lines(mounts),
            network: network.checked,
            max_seconds: Number(seconds.value),
            enabled: enabled.checked,
        };
        const next = structuredClone(config);
        next.adapters = next.adapters.filter((a) => a.id !== old?.id);
        next.adapters.push(value);
        await save(next);
    });
}
