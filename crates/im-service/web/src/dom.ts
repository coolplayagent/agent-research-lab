import { label } from "./i18n.js";
import { busy } from "./feedback.js";
export function get<T extends HTMLElement = HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`Missing interface element: ${id}`);
  return node as T;
}
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text?: string,
  className?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
export function button(text: string, action: () => void | Promise<void>): HTMLButtonElement {
  const node = el("button", text);
  node.type = "button";
  node.onclick = async () => {
    const done = busy(node);
    if (!done) return;
    try { await action(); } catch (error) { notice(error); } finally { done(); }
  };
  return node;
}
export function actions(...controls: HTMLElement[]): HTMLDivElement {
  const row = el("div", undefined, "card-actions");
  row.append(...controls);
  return row;
}
export const value = (id: string): string =>
  get<HTMLInputElement>(id).value.trim();
export function notice(error: unknown, id = "notice"): void {
  get(id).textContent = error instanceof Error ? error.message : String(error);
  if (id === "notice") get("dismiss-notice").hidden = !get(id).textContent;
}
export function open(id: string): void {
  get<HTMLDialogElement>(id).showModal();
}
export function close(id: string): void {
  get<HTMLDialogElement>(id).close();
}
export function initDialogs(): void {
  document
    .querySelectorAll<HTMLElement>("[data-close]")
    .forEach((node) => (node.onclick = () => close(node.dataset.close!)));
}
export const presence = (value: string): string => label("Presence", value);
