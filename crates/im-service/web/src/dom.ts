import { label } from "./i18n.js";
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
export function button(text: string, action: () => void): HTMLButtonElement {
  const node = el("button", text);
  node.type = "button";
  node.onclick = action;
  return node;
}
export const value = (id: string): string =>
  get<HTMLInputElement>(id).value.trim();
export function notice(error: unknown, id = "notice"): void {
  get(id).textContent = error instanceof Error ? error.message : String(error);
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
  get<HTMLDialogElement>("credential-dialog").addEventListener("close", () => {
    get<HTMLTextAreaElement>("credential-value").value = "";
  });
}
export const presence = (value: string): string => label("Presence", value);
