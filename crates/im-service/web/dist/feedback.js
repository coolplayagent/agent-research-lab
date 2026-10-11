import { t } from "./i18n.js";
/** A button owns its pending operation, including programmatic repeated submissions. */
export function busy(button) {
    if (button.getAttribute("aria-busy") === "true")
        return null;
    const text = button.textContent;
    const plain = button.childElementCount === 0;
    const disabled = button.disabled;
    button.disabled = true;
    button.setAttribute("aria-busy", "true");
    if (plain)
        button.textContent = t("ux.working");
    return () => {
        if (plain)
            button.textContent = text;
        button.disabled = disabled;
        button.removeAttribute("aria-busy");
    };
}
