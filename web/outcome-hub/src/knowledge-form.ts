import { KnowledgeBackend, KnowledgeAccess } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { button, el, actions } from "./dom.js";
import { input, check, area, field, dialog, finish } from "./forms.js";
import { trackSettings } from "./settings-guard.js";
import type { KnowledgeTarget } from "./knowledge-settings.js";

export function knowledgeForm(kind: KnowledgeBackend, save: (name: string, target: KnowledgeTarget) => Promise<void>, inspect: (target: KnowledgeTarget) => Promise<{ target: KnowledgeTarget; revision: string }>, name?: string, old?: KnowledgeTarget): void {
  const box = dialog(t(kind === KnowledgeBackend.Git ? "knowledge.addGit" : "knowledge.addDirectory"));
  const id = input(name || ""), path = input(old?.path || ""), root = input(old?.content_root || "."),
    writable = check(old?.access === KnowledgeAccess.ReadWrite), enabled = check(old?.enabled ?? true),
    branch = input(old?.reference || ""), remote = input(old?.remote || ""), env = area(old?.credential_env.join("\n") || "");
  id.required = path.required = true; id.readOnly = !!name; id.pattern = "[A-Za-z0-9_-]{1,100}";
  id.placeholder = "team-notes";
  branch.placeholder = t("knowledge.detectBranch");
  box.form.append(field(t("knowledge.id"), id), el("p", t("knowledge.idNote")), field(t("knowledge.path"), path), field(t("knowledge.writable"), writable));
  const advanced = el("details"), summary = el("summary", t("settings.advanced"));
  advanced.append(summary, field(t("knowledge.root"), root), el("p", t("knowledge.rootNote")), field(t("outcome.enabled"), enabled));
  if (kind === KnowledgeBackend.Git) advanced.append(field(t("knowledge.branch"), branch), el("p", t("knowledge.branchNote")), field(t("knowledge.remote"), remote), field(t("knowledge.credentials"), env));
  const update = () => { branch.required = kind === KnowledgeBackend.Git && writable.checked; if (branch.required) advanced.open = true; };
  writable.onchange = update; update();
  const target = (): KnowledgeTarget => ({ kind, path: path.value.trim(), content_root: root.value.trim() || ".", enabled: enabled.checked,
    access: writable.checked ? KnowledgeAccess.ReadWrite : KnowledgeAccess.ReadOnly,
    reference: kind === KnowledgeBackend.Git ? branch.value.trim() || null : null,
    remote: kind === KnowledgeBackend.Git ? remote.value.trim() || null : null,
    credential_env: kind === KnowledgeBackend.Git ? env.value.split("\n").map(v => v.trim()).filter(Boolean) : [] });
  const probe = button(t("knowledge.checkPath"), async () => {
    box.status.textContent = t("knowledge.checkingPath");
    const checked = target();
    try {
      const result = await inspect(checked);
      if (JSON.stringify(checked) !== JSON.stringify(target())) return;
      branch.value = result.target.reference || "";
      box.status.textContent = t("knowledge.pathReady", { revision: result.revision });
    } catch (error) { box.status.textContent = t("knowledge.pathFailed", { error: String(error) }); }
  });
  box.form.append(advanced, actions(probe));
  trackSettings(box.form);
  finish(box, async () => { await save(id.value.trim(), target()); });
}
