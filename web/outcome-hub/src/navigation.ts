import { api, query } from "./api.js";
import { select } from "./conversation.js";
import type { GroupView } from "./types.js";
import type { OutcomeView } from "./outcome-types.js";
import type { OutcomeTask } from "./task-types.js";
import { revealResearch } from "./workspace.js";
import { loadTasks, loadCoordinator } from "./tasks.js";
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
  people: t("workspace.people"),
};
const settings: Record<string, string> = {
  services: t("workspace.services"),
  knowledge: t("knowledge.title"),
  adapters: t("workspace.adapters"),
  bindings: t("workspace.bindings"),
  coordinator: t("tasks.coordinator"),
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
  if (!requested || requested.startsWith("topics") || ["evolution", "iterations"].includes(requested)) { requested = "groups"; history.replaceState(null, "", "#groups"); }
  const page = setting ? "settings" : titles[requested.split("/")[0]] ? requested.split("/")[0] : "groups";
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
  for (const name of ["people", "settings"])
    get(name + "-page").hidden = name !== page;
  get("conversations").hidden = !conversation;
  if (conversation) {
    get("messages").scrollTop = scroll;
    workspaceRoute = "#" + page;
    get<HTMLAnchorElement>("return-workspace").href = workspaceRoute;
  }
  if (requested.startsWith("tasks/")) void redirectTask(requested.slice(6)).catch(notice);
  else if (/^groups\/[^/]+\/tasks(?:\/[^/]+)?(?:\/outcomes\/[^/]+)?$/.test(requested)) void openBoard(requested).catch(notice);
  else if (requested === "tasks") { history.replaceState(null, "", "#groups"); }
  else void loadTasks();
  if (requested.startsWith("outcomes/")) {
    void redirectOutcome(requested.slice("outcomes/".length)).catch(notice);
  } else document.getElementById("outcome-dialog")?.remove();
  if (page === "people") void loadPeople();
  if (setting === "coordinator") void loadCoordinator();
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

async function redirectTask(id: string): Promise<void> {
  const route = location.hash;
  const task = await api<OutcomeTask>("/api/v2/collaboration/tasks?" + query({ id: decodeURIComponent(id) }));
  if (location.hash === route) location.replace("#groups/" + encodeURIComponent(task.input.group_id) + "/tasks/" + encodeURIComponent(task.input.id));
}
async function openBoard(route: string): Promise<void> {
  const groupId = decodeURIComponent(route.split("/")[1]);
  const view = await api<GroupView>("/api/v1/messaging/view?" + query({ group_id: groupId }));
  if (location.hash.slice(1) !== route) return;
  await select(view.group);
  if (location.hash.slice(1) !== route) return;
  revealResearch("goals"); await loadTasks();
  const artifact = route.split("/")[5];
  if (artifact && location.hash.slice(1) === route) await showOutcome(decodeURIComponent(artifact));
}
async function redirectOutcome(id: string): Promise<void> {
  const route = location.hash;
  const view = await api<OutcomeView>("/api/v2/collaboration/outcomes?" + query({ id: decodeURIComponent(id) }));
  if (location.hash !== route) return;
  const entry = view.outcome.input;
  location.replace("#groups/" + encodeURIComponent(entry.group_id) + "/tasks/" + encodeURIComponent(entry.goal_id) + "/outcomes/" + id);
}
