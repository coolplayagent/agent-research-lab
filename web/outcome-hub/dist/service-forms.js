import { trackSettings } from "./settings-guard.js";
import { ServiceKind, ServiceManagement } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { select, input, check, dialog, finish, field, element } from "./forms.js";
export function editService(config, old, save) {
    const box = dialog(t("services.editService")), id = input(old?.id || ""), kind = select(Object.values(ServiceKind).map(value => [value, label("ServiceKind", value)]), old?.kind || ServiceKind.Executor), management = select(Object.values(ServiceManagement).map(value => [value, label("ServiceManagement", value)]), old?.management || ServiceManagement.External), endpoint = input(old?.endpoint || ""), enabled = check(old?.enabled ?? true), sandbox = select([["", t("services.none")], ...config.services.filter(s => s.kind === ServiceKind.Sandbox && s.enabled).map(s => [s.id, s.id])], old?.sandbox_id || "");
    id.required = true;
    id.readOnly = !!old;
    kind.disabled = !!old;
    endpoint.required = true;
    endpoint.readOnly = old?.management === ServiceManagement.LocalManaged;
    box.form.append(field(t("services.id"), id), field(t("services.kind"), kind), element("p", t("settings.serviceRoleNote")), field(t("settings.management"), management), field(t("services.endpoint"), endpoint), field(t("services.enabled"), enabled), field(t("services.sandbox"), sandbox));
    const dependency = sandbox.closest("label"), note = element("p", t("settings.noSandbox"));
    box.form.append(note);
    const update = () => {
        const executor = kind.value === ServiceKind.Executor;
        management.disabled = kind.value === ServiceKind.Storage;
        if (management.disabled)
            management.value = ServiceManagement.External;
        endpoint.readOnly = management.value === ServiceManagement.LocalManaged && !!old;
        dependency.hidden = !executor;
        sandbox.required = executor && enabled.checked;
        note.hidden = !executor || !enabled.checked || config.services.some(s => s.kind === ServiceKind.Sandbox && s.enabled);
    };
    kind.onchange = update;
    management.onchange = update;
    enabled.onchange = update;
    update();
    trackSettings(box.form);
    finish(box, async () => {
        if (kind.value === ServiceKind.Executor && enabled.checked && !sandbox.value)
            throw new Error(t("settings.noSandbox"));
        const value = { id: id.value.trim(), kind: old?.kind || kind.value,
            endpoint: endpoint.value.trim(), enabled: enabled.checked, management: management.value,
            sandbox_id: kind.value === ServiceKind.Executor ? sandbox.value || null : null };
        const next = structuredClone(config);
        next.services = next.services.filter(s => s.id !== old?.id);
        next.services.push(value);
        await save(next);
    });
}
