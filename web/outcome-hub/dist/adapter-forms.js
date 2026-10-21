import { trackSettings, allowSettingsLeave } from "./settings-guard.js";
import { CodingAgent, AgentProtocol, ServiceKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { element, select, input, check, area, dialog, finish, field } from "./forms.js";
import { chooseBinding } from "./binding-settings.js";
export function editAdapter(config, old, save) {
    const box = dialog(t("services.editAdapter")), id = input(old?.id || ""), agent = select(Object.values(CodingAgent).map(value => [value, label("CodingAgent", value)]), old?.agent || CodingAgent.Codex), executor = select(config.services.filter(s => s.kind === ServiceKind.Executor && s.enabled).map(s => [s.id, s.id]), old?.executor_id || "executor"), command = area(JSON.stringify(old?.command || [], null, 2)), env = area((old?.env_names || []).join("\n")), mounts = area((old?.read_only_paths || []).join("\n")), enabled = check(old?.enabled ?? false), network = check(old?.network ?? true), seconds = input(String(old?.max_seconds || 1800), "number");
    id.required = true;
    id.readOnly = !!old;
    agent.disabled = !!old;
    executor.required = true;
    seconds.min = "1";
    seconds.max = "3600";
    seconds.required = true;
    const builtin = element("section"), custom = element("section");
    builtin.append(element("p", t("settings.builtinNote")));
    if (old) {
        const link = element("button", t("binding.validate"));
        link.type = "button";
        link.onclick = () => { if (allowSettingsLeave()) {
            box.dialog.close();
            chooseBinding(old.id);
        } };
        builtin.append(link);
        if (old.command.length) {
            const generated = area(JSON.stringify(old.command, null, 2));
            generated.readOnly = true;
            builtin.append(field(t("settings.generatedCommand"), generated));
        }
    }
    custom.append(field(t("services.command"), command), element("p", t("services.commandNote")), field(t("services.env"), env), element("p", t("services.envNote")), field(t("services.mounts"), mounts), field(t("services.network"), network), field(t("services.enabled"), enabled));
    box.form.append(field(t("services.id"), id), field(t("services.agent"), agent), field(t("services.executor"), executor), field(t("services.limit"), seconds), builtin, custom);
    const update = () => { const native = agent.value !== CodingAgent.Custom; builtin.hidden = !native; custom.hidden = native; };
    agent.onchange = update;
    update();
    trackSettings(box.form);
    finish(box, async () => {
        const lines = (node) => node.value.split("\n").map(s => s.trim()).filter(Boolean);
        const native = agent.value !== CodingAgent.Custom;
        const argv = native ? old?.command || [] : JSON.parse(command.value);
        if (!Array.isArray(argv) || !argv.every(v => typeof v === "string"))
            throw new Error(t("services.commandInvalid"));
        if (!native && enabled.checked && !argv.length)
            throw new Error(t("api.adapter_command_required"));
        const unchanged = !!old && old.executor_id === executor.value && old.max_seconds === Number(seconds.value);
        const value = { id: id.value.trim(), agent: agent.value, protocol: AgentProtocol.JsonStdioV1,
            executor_id: executor.value, max_seconds: Number(seconds.value),
            enabled: native ? unchanged && !!old?.enabled : enabled.checked,
            command: native && !unchanged ? [] : argv,
            env_names: native ? old?.env_names || [] : lines(env), read_only_paths: native ? old?.read_only_paths || [] : lines(mounts),
            network: native ? old?.network ?? true : network.checked };
        const next = structuredClone(config);
        next.adapters = next.adapters.filter(a => a.id !== old?.id);
        next.adapters.push(value);
        await save(next);
        if (native && !value.enabled)
            chooseBinding(value.id);
    });
}
