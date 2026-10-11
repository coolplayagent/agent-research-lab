import { t } from "./i18n.js";
const dirty = new Set<HTMLFormElement>();
export function trackSettings(form: HTMLFormElement): void {
  form.addEventListener("input", () => dirty.add(form));
  form.addEventListener("change", () => dirty.add(form));
  form.addEventListener("settings-saved", () => dirty.delete(form));
}
export function savedSettings(form: HTMLFormElement): void {
  dirty.delete(form);
}
export function hasUnsavedSettings(): boolean {
  for (const form of dirty) if (!form.isConnected) dirty.delete(form);
  return dirty.size > 0;
}
export function allowSettingsLeave(): boolean {
  if (!hasUnsavedSettings()) return true;
  if (!window.confirm(t("workspace.unsaved"))) return false;
  for (const form of dirty) form.reset();
  dirty.clear();
  return true;
}
export function initSettingsGuard(): void {
  window.addEventListener("beforeunload", (event) => {
    if (hasUnsavedSettings()) event.preventDefault();
  });
}
