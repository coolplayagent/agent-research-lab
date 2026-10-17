import { label } from "./i18n.js";
import { busy } from "./feedback.js";
export function get(id) {
    const node = document.getElementById(id);
    if (!node)
        throw new Error(`Missing interface element: ${id}`);
    return node;
}
export function el(tag, text, className) {
    const node = document.createElement(tag);
    if (text !== undefined)
        node.textContent = text;
    if (className)
        node.className = className;
    return node;
}
export function button(text, action) {
    const node = el("button", text);
    node.type = "button";
    node.onclick = async () => {
        const done = busy(node);
        if (!done)
            return;
        try {
            await action();
        }
        catch (error) {
            notice(error);
        }
        finally {
            done();
        }
    };
    return node;
}
export function actions(...controls) {
    const row = el("div", undefined, "card-actions");
    row.append(...controls);
    return row;
}
export const value = (id) => get(id).value.trim();
export function notice(error, id = "notice") {
    get(id).textContent = error instanceof Error ? error.message : String(error);
    if (id === "notice")
        get("dismiss-notice").hidden = !get(id).textContent;
}
export function open(id) {
    get(id).showModal();
}
export function close(id) {
    get(id).close();
}
export function initDialogs() {
    document
        .querySelectorAll("[data-close]")
        .forEach((node) => (node.onclick = () => close(node.dataset.close)));
}
export const presence = (value) => label("Presence", value);
