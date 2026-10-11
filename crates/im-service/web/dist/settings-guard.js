import { t } from "./i18n.js";
const dirty = new Set();
export function trackSettings(form) {
    form.addEventListener("input", () => dirty.add(form));
    form.addEventListener("change", () => dirty.add(form));
    form.addEventListener("settings-saved", () => dirty.delete(form));
}
export function savedSettings(form) {
    dirty.delete(form);
}
export function hasUnsavedSettings() {
    for (const form of dirty)
        if (!form.isConnected)
            dirty.delete(form);
    return dirty.size > 0;
}
export function allowSettingsLeave() {
    if (!hasUnsavedSettings())
        return true;
    if (!window.confirm(t("workspace.unsaved")))
        return false;
    for (const form of dirty)
        form.reset();
    dirty.clear();
    return true;
}
export function initSettingsGuard() {
    window.addEventListener("beforeunload", (event) => {
        if (hasUnsavedSettings())
            event.preventDefault();
    });
}
