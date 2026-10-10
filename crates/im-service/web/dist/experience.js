import { get, open, close } from "./dom.js";
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
export function showConversation(show) {
    get("conversations").classList.toggle("show-conversation", show);
}
export function initExperience() {
    const menu = get("toggle-navigation");
    const backdrop = get("navigation-backdrop");
    const mobile = matchMedia("(max-width: 900px)");
    function navigation(show) {
        get("workspace").classList.toggle("navigation-open", show);
        menu.setAttribute("aria-expanded", String(show));
        menu.setAttribute("aria-label", t(show ? "ux.closeMenu" : "ux.menu"));
        backdrop.hidden = !show;
        get("workspace").querySelector("main").inert = show && mobile.matches;
        if (show)
            get("new-conversation").focus();
    }
    menu.onclick = () => navigation(menu.getAttribute("aria-expanded") !== "true");
    backdrop.onclick = () => { navigation(false); menu.focus(); };
    window.addEventListener("hashchange", () => navigation(false));
    document.addEventListener("keydown", (event) => {
        if (event.key === "Escape" && menu.getAttribute("aria-expanded") === "true") {
            navigation(false);
            menu.focus();
        }
    });
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
    // The closed mobile navigation must not leave invisible controls in the tab order.
    const sync = () => {
        if (!mobile.matches)
            navigation(false);
    };
    mobile.addEventListener("change", sync);
    document.querySelectorAll("[data-page]").forEach((button) => {
        button.addEventListener("click", () => {
            if (mobile.matches) {
                navigation(false);
                get("page-title").focus({ preventScroll: true });
            }
        });
    });
    get("workspace-navigation").addEventListener("keydown", (event) => {
        if (event.key !== "Tab" || !mobile.matches || menu.getAttribute("aria-expanded") !== "true")
            return;
        const items = [...get("workspace-navigation").querySelectorAll("a[href], button:not(:disabled)")];
        const first = items[0], last = items.at(-1);
        if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last?.focus();
        }
        else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first?.focus();
        }
    });
    get("back-conversations").onclick = () => {
        showConversation(false);
        get("group-list").querySelector(".selected")?.focus();
    };
    get("open-guide").onclick = () => { navigation(false); open("guide-dialog"); };
    for (const id of ["welcome-create", "guide-create"]) {
        get(id).onclick = () => {
            close("guide-dialog");
            navigation(false);
            get("new-conversation").click();
        };
    }
    for (const id of ["welcome-people", "guide-people"]) {
        get(id).onclick = () => { close("guide-dialog"); location.hash = "people"; };
    }
    get("guide-applications").onclick = () => { close("guide-dialog"); location.hash = "applications"; };
    document.querySelectorAll("[data-settings-target]").forEach((button) => {
        button.onclick = () => {
            const target = get(button.dataset.settingsTarget);
            target.scrollIntoView({ block: "start" });
            target.querySelector("h3")?.focus({ preventScroll: true });
        };
    });
    get("dismiss-notice").onclick = () => {
        get("notice").textContent = "";
        get("dismiss-notice").hidden = true;
    };
    searchKeys(get("people-search"), () => get("search-people").click());
    searchKeys(get("topic-search"), () => get("topic-search-button").click());
}
