import { KnowledgeBackend, KnowledgeAccess } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { button, el, actions } from "./dom.js";
import { input, check, area, field, dialog, finish } from "./forms.js";
import { trackSettings } from "./settings-guard.js";
export function knowledgeForm(kind, save, inspect, name, old) {
    const box = dialog(t(name ? "knowledge.edit" : "knowledge.add"));
    box.dialog.className = "knowledge-dialog";
    const id = input(name || ""), path = input(old?.path || ""), root = input(old?.content_root || "."), writable = check(old?.access === KnowledgeAccess.ReadWrite), enabled = check(old?.enabled ?? true), branch = input(old?.reference || ""), remote = input(old?.remote || ""), env = area(old?.credential_env.join("\n") || "");
    id.required = path.required = true;
    id.readOnly = !!name;
    id.pattern = "[A-Za-z0-9_\\-]{1,100}";
    id.placeholder = "team-notes";
    branch.placeholder = t("knowledge.detectBranch");
    const types = el("fieldset", undefined, "knowledge-types"), git = el("section", undefined, "knowledge-git-options");
    types.append(el("legend", t("knowledge.kind")));
    const choices = [KnowledgeBackend.Git, KnowledgeBackend.Directory].map(value => {
        const radio = input(value, "radio");
        radio.name = "knowledge-kind";
        radio.checked = value === kind;
        const choice = el("label", undefined, "knowledge-type"), text = el("span");
        text.append(el("strong", t(value === KnowledgeBackend.Git ? "knowledge.gitType" : "knowledge.directoryType")), el("small", t(value === KnowledgeBackend.Git ? "knowledge.gitDescription" : "knowledge.directoryDescription")));
        choice.append(radio, text);
        types.append(choice);
        radio.onchange = () => { kind = value; update(); };
        return radio;
    });
    const basic = el("div", undefined, "knowledge-form-grid");
    basic.append(field(t("knowledge.id"), id), field(t("knowledge.path"), path));
    const advanced = el("details", undefined, "knowledge-advanced");
    advanced.append(el("summary", t("knowledge.advancedOptions")), field(t("knowledge.root"), root), el("p", t("knowledge.rootNote")), field(t("knowledge.enabledOption"), enabled));
    git.append(field(t("knowledge.branch"), branch), el("p", t("knowledge.branchNote")), field(t("knowledge.remote"), remote), field(t("knowledge.credentials"), env));
    advanced.append(git);
    const update = () => {
        for (const radio of choices)
            radio.checked = radio.value === kind;
        git.hidden = kind !== KnowledgeBackend.Git;
        branch.disabled = remote.disabled = env.disabled = git.hidden;
        branch.required = kind === KnowledgeBackend.Git && writable.checked;
        if (branch.required)
            advanced.open = true;
        box.status.textContent = "";
    };
    writable.onchange = update;
    update();
    const target = () => ({ kind, path: path.value.trim(), content_root: root.value.trim() || ".", enabled: enabled.checked,
        access: writable.checked ? KnowledgeAccess.ReadWrite : KnowledgeAccess.ReadOnly,
        reference: kind === KnowledgeBackend.Git ? branch.value.trim() || null : null,
        remote: kind === KnowledgeBackend.Git ? remote.value.trim() || null : null,
        credential_env: kind === KnowledgeBackend.Git ? env.value.split("\n").map(v => v.trim()).filter(Boolean) : [] });
    let generation = 0;
    box.form.addEventListener("input", () => { generation++; box.status.textContent = ""; });
    const probe = button(t("knowledge.checkPath"), async () => {
        const request = ++generation, checked = target();
        box.status.textContent = t("knowledge.checkingPath");
        try {
            const result = await inspect(checked);
            if (request !== generation || JSON.stringify(checked) !== JSON.stringify(target()))
                return;
            branch.value = result.target.reference || "";
            box.status.textContent = t("knowledge.pathReady", { revision: result.revision });
        }
        catch (error) {
            if (request === generation && JSON.stringify(checked) === JSON.stringify(target()))
                box.status.textContent = t("knowledge.pathFailed", { error: String(error) });
        }
    });
    box.form.append(el("p", t("knowledge.formNote"), "knowledge-form-note"), types, basic, el("p", t("knowledge.idNote")), field(t("knowledge.writable"), writable), advanced, actions(probe));
    trackSettings(box.form);
    finish(box, async () => { await save(id.value.trim(), target()); });
}
