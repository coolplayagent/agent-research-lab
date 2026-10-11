import { get } from "./dom.js";
import { t } from "./i18n.js";
/** Search controls behave consistently without submitting an enclosing form. */
export function searchKeys(input, search) {
    input.addEventListener("keydown", (event) => {
        if (event.isComposing)
            return;
        if (event.key === "Enter") {
            event.preventDefault();
            search();
        }
        else if (event.key === "Escape" && input.value) {
            event.preventDefault();
            input.value = "";
            search();
        }
    });
    input.addEventListener("search", search);
}
export function initExperience() {
    document.addEventListener("input", (event) => {
        const input = event.target;
        if (input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement || input instanceof HTMLSelectElement)
            input.setCustomValidity("");
    });
    document.addEventListener("invalid", (event) => {
        const input = event.target;
        if (!(input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement || input instanceof HTMLSelectElement))
            return;
        if (input.validity.valueMissing)
            input.setCustomValidity(t("ux.required"));
        else if (input.validity.patternMismatch)
            input.setCustomValidity(t("ux.invalidId"));
        else if (input instanceof HTMLInputElement && (input.validity.rangeUnderflow || input.validity.rangeOverflow)) {
            input.setCustomValidity(t("ux.range", { min: input.min, max: input.max }));
        }
    }, true);
    document.addEventListener("submit", (event) => {
        const form = event.target;
        if (!(form instanceof HTMLFormElement))
            return;
        for (const input of form.querySelectorAll("input[required], textarea[required]")) {
            if (!input.disabled && !input.value.trim()) {
                input.setCustomValidity(t("ux.required"));
                event.preventDefault();
                event.stopImmediatePropagation();
                input.reportValidity();
                break;
            }
        }
    }, true);
    get("dismiss-notice").onclick = () => {
        get("notice").textContent = "";
        get("dismiss-notice").hidden = true;
    };
    searchKeys(get("people-search"), () => get("search-people").click());
    searchKeys(get("topic-search"), () => get("topic-search-button").click());
}
