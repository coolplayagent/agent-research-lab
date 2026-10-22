import { get } from "./dom.js";
import { t } from "./i18n.js";

let tab = "collaboration";
export function selectTab(name: string): void {
  if (!document.querySelector(`[data-tab="${name}"]`)) return;
  tab = name;
  for (const item of document.querySelectorAll<HTMLButtonElement>(
    "[data-tab]",
  )) {
    const selected = item.dataset.tab === tab;
    item.setAttribute("aria-selected", String(selected));
    item.tabIndex = selected ? 0 : -1;
    const panel = get(item.getAttribute("aria-controls")!);
    panel.hidden = !selected;
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", item.id);
  }
}
export function revealResearch(name: string): void {
  const root = get("workspace");
  root.classList.remove("directory-open");
  root.classList.remove("right-collapsed");
  root.classList.add("research-open");
  selectTab(name);
  syncControls();
}
export function syncControls(): void {
  const root = get("workspace");
  const left = matchMedia("(max-width: 759px)").matches
    ? root.classList.contains("directory-open")
    : !root.classList.contains("left-collapsed");
  const right = matchMedia("(max-width: 1099px)").matches
    ? root.classList.contains("research-open")
    : !root.classList.contains("right-collapsed");
  const expanded = root.classList.contains("research-expanded");
  get("expand-research").setAttribute("aria-pressed", String(expanded));
  get("expand-research").textContent = expanded ? t("workspace.restore") : t("workspace.expand");
  get("toggle-directory").setAttribute("aria-expanded", String(left));
  get("toggle-research").setAttribute("aria-expanded", String(right));
}
function resize(
  side: "left" | "right",
  initial: number,
  min: number,
  max: number,
): void {
  const handle = get("resize-" + side);
  let width = initial;
  const update = (value: number) => {
    width = Math.round(Math.max(min, Math.min(max, value)));
    get("workspace").style.setProperty(`--${side}-width`, width + "px");
    handle.setAttribute("aria-valuenow", String(width));
  };
  handle.setAttribute("aria-valuemin", String(min));
  handle.setAttribute("aria-valuemax", String(max));
  update(width);
  handle.onpointerdown = (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const start = event.clientX,
      original = width;
    handle.setPointerCapture(event.pointerId);
    handle.onpointermove = (move) =>
      update(original + (move.clientX - start) * (side === "left" ? 1 : -1));
    handle.onpointerup = handle.onpointercancel = () => {
      handle.onpointermove = null;
      if (handle.hasPointerCapture(event.pointerId))
        handle.releasePointerCapture(event.pointerId);
    };
  };
  handle.onkeydown = (event) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    update(
      event.key === "Home"
        ? min
        : event.key === "End"
          ? max
          : width +
            (event.key === "ArrowRight" ? 10 : -10) *
              (side === "left" ? 1 : -1),
    );
  };
}
export function initWorkspace(): void {
  const root = get("workspace");
  const tabs = [...document.querySelectorAll<HTMLButtonElement>("[data-tab]")];
  tabs.forEach((item, index) => {
    item.onclick = () => selectTab(item.dataset.tab!);
    item.onkeydown = (event) => {
      let next = index;
      if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
      else if (event.key === "ArrowLeft")
        next = (index + tabs.length - 1) % tabs.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else return;
      event.preventDefault();
      tabs[next].focus();
      selectTab(tabs[next].dataset.tab!);
    };
  });
  selectTab(tab);
  get("toggle-directory").onclick = () => {
    root.classList.toggle(
      matchMedia("(max-width: 759px)").matches
        ? "directory-open"
        : "left-collapsed",
    );
    if (matchMedia("(max-width: 759px)").matches && root.classList.contains("directory-open")) {
      root.classList.remove("research-open", "research-expanded");
    }
    syncControls();
  };
  get("toggle-research").onclick = () => {
    root.classList.remove("directory-open");
    if (matchMedia("(max-width: 1099px)").matches) {
      root.classList.remove("right-collapsed");
      root.classList.toggle("research-open");
    } else root.classList.toggle("right-collapsed");
    syncControls();
  };
  get("close-research").onclick = () => {
    root.classList.remove("research-open", "research-expanded");
    root.classList.add("right-collapsed");
    syncControls();
    get("toggle-research").focus();
  };
  get("expand-research").onclick = () => {
    root.classList.toggle("research-expanded");
    syncControls();
  };
  get("goal-summary").onclick = () => revealResearch("goals");
  get("research-manage-members").onclick = () => get("manage-members").click();
  window.addEventListener("resize", syncControls);
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || document.querySelector("dialog[open]"))
      return;
    root.classList.remove(
      "directory-open",
      "research-open",
      "research-expanded",
    );
    syncControls();
  });
  resize("left", 240, 200, 300);
  resize("right", 360, 320, 460);
  syncControls();
}
