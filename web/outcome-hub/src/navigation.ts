import { loadTopics } from "./topics.js";
import { showOutcome } from "./outcome-detail.js";
import { notice } from "./dom.js";
import { loadServices } from "./settings.js";
import { resetBindingProgress } from "./binding-settings.js";
import { t } from "./i18n.js";
import { get } from "./dom.js";
import { icon } from "./icons.js";
import { loadPeople } from "./people.js";
import { allowSettingsLeave, initSettingsGuard } from "./settings-guard.js";
import { syncControls } from "./workspace.js";
const titles: Record<string, string> = {
  groups: t("navigation.f3f8bcf3f5"),
  direct: t("navigation.484c0ffc97"),
  boards: t("navigation.ca4773c5ff"),
  topics: t("research.topics"),
  people: t("workspace.people"),
};
const settings: Record<string, string> = {
  services: t("workspace.services"),
  knowledge: t("knowledge.title"),
  adapters: t("workspace.adapters"),
  bindings: t("workspace.bindings"),
  storage: t("workspace.storage"),
};
let previous = "",
  workspaceRoute = "#groups",
  scroll = 0;
export function navigate(): void {
  let requested = location.hash.slice(1);
  if (["applications", "evolution", "iterations"].includes(requested)) { requested = "groups"; history.replaceState(null, "", "#groups"); }
  if (
    requested === "settings" ||
    (requested.startsWith("settings/") && !settings[requested.split("/")[1]])
  ) {
    requested = "settings/bindings";
    history.replaceState(null, "", "#" + requested);
  }
  if (
    previous.startsWith("#settings/") &&
    location.hash !== previous &&
    !allowSettingsLeave()
  ) {
    history.replaceState(null, "", previous);
    return;
  }
  if (previous !== "#settings/bindings" || location.hash !== "#settings/bindings") resetBindingProgress();
  const setting = requested.startsWith("settings/")
    ? requested.split("/")[1]
    : "";
  const page = setting
    ? "settings"
    : requested.startsWith("topics/outcomes/")
      ? "topics"
      : titles[requested]
        ? requested
        : "groups";
  const conversation = ["groups", "direct", "boards"].includes(page);
  if (!get("conversations").hidden) scroll = get("messages").scrollTop;
  get("toggle-directory").textContent = setting
    ? t("workspace.settingsNav")
    : t("workspace.conversations");
  get("page-title").textContent = setting ? settings[setting] : titles[page];
  get("workspace-navigation").hidden = !!setting;
  get("settings-navigation").hidden = !setting;
  get("workspace").classList.toggle("auxiliary", !conversation);
  if (!conversation) {
    get("workspace").classList.remove("research-expanded", "research-open");
    get("expand-research").setAttribute("aria-pressed", "false");
    get("expand-research").textContent = t("workspace.expand");
  }
  if (setting) get("workspace").classList.remove("left-collapsed");
  for (const name of ["people", "settings", "topics"])
    get(name + "-page").hidden = name !== page;
  get("conversations").hidden = !conversation;
  if (conversation) {
    get("messages").scrollTop = scroll;
    workspaceRoute = "#" + page;
    get<HTMLAnchorElement>("return-workspace").href = workspaceRoute;
  }
  if (page === "topics") void loadTopics();
  if (requested.startsWith("topics/outcomes/")) {
    void showOutcome(decodeURIComponent(requested.slice("topics/outcomes/".length))).catch(notice);
  } else document.getElementById("outcome-dialog")?.remove();
  if (page === "people") void loadPeople();
  if (setting) {
    if (["services", "adapters", "storage"].includes(setting))
      get<HTMLDetailsElement>("advanced-settings").open = true;
    for (const panel of document.querySelectorAll<HTMLElement>(
      "[data-settings-panel]",
    ))
      panel.hidden = panel.dataset.settingsPanel !== setting;
    for (const link of document.querySelectorAll<HTMLElement>("[data-setting]"))
      link.setAttribute(
        "aria-current",
        link.dataset.setting === setting ? "page" : "false",
      );
    void loadServices();
  }
  document
    .querySelectorAll<HTMLElement>("[data-page]")
    .forEach((node) =>
      node.setAttribute(
        "aria-current",
        node.dataset.page === page ? "page" : "false",
      ),
    );
  previous = location.hash;
  syncControls();
}
export function initNavigation(): void {
  get("topic-nav-icon").replaceChildren(icon("topic"));
  get("people-nav-icon").replaceChildren(icon("people"));
  initSettingsGuard();
  document.addEventListener("click", (event) => {
    const link = (event.target as Element).closest<HTMLAnchorElement>(
      "a[href^='#']",
    );
    if (!link) return;
    if (
      location.hash.startsWith("#settings/") &&
      link.hash !== location.hash &&
      !allowSettingsLeave()
    )
      event.preventDefault();
    else get("workspace").classList.remove("directory-open");
  });
  window.addEventListener("hashchange", navigate);
  navigate();
}
