import { busy } from "./feedback.js";
import { t } from "./i18n.js";
export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  return node;
}
export function select(
  options: [string, string][],
  value: string,
): HTMLSelectElement {
  const node = element("select");
  for (const [id, title] of options) {
    const option = element("option", title);
    option.value = id;
    node.append(option);
  }
  node.value = value;
  return node;
}
export function input(value: string, type = "text"): HTMLInputElement {
  const node = element("input");
  node.type = type;
  node.value = value;
  return node;
}
export function check(value: boolean): HTMLInputElement {
  const node = input("", "checkbox");
  node.checked = value;
  return node;
}
export function area(value: string): HTMLTextAreaElement {
  const node = element("textarea");
  node.rows = 3;
  node.value = value;
  return node;
}
export function field(title: string, node: HTMLElement): HTMLLabelElement {
  const label = element("label");
  node.setAttribute("aria-label", title);
  label.append(element("span", title), node);
  return label;
}
export function dialog(title: string): {
  dialog: HTMLDialogElement;
  form: HTMLFormElement;
  status: HTMLElement;
} {
  const dialog = element("dialog"),
    form = element("form"),
    header = element("header"),
    close = element("button", t("services.close"));
  close.type = "button";
  close.onclick = () => dialog.close();
  const heading = element("h2", title);
  heading.id = "dialog-" + crypto.randomUUID();
  dialog.setAttribute("aria-labelledby", heading.id);
  header.append(heading, close);
  form.append(header);
  const status = element("p");
  status.setAttribute("role", "status");
  dialog.append(form);
  document.body.append(dialog);
  dialog.onclose = () => dialog.remove();
  return { dialog, form, status };
}
export function finish(
  box: ReturnType<typeof dialog>,
  save: () => Promise<void>,
): void {
  const button = element("button", t("services.save"));
  button.type = "submit";
  button.className = "primary";
  box.form.append(box.status, button);
  box.form.onsubmit = async (event) => {
    event.preventDefault();
    const done = busy(button);
    if (!done) return;
    box.status.textContent = "";
    try {
      await save();
      box.dialog.close();
    } catch (error) {
      box.status.textContent = String(error);
    } finally {
      done();
    }
  };
  box.dialog.showModal();
}
