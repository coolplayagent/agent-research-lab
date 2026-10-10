import { loadServices } from "./settings.js";
import { t } from "./i18n.js";
import { get } from "./dom.js";
import { setSection, type Section } from "./directory.js";
import { loadPeople } from "./people.js";
import { loadApplications } from "./applications.js";
const titles: Record<string, string> = {
  groups: t("navigation.f3f8bcf3f5"),
  direct: t("navigation.484c0ffc97"),
  boards: t("navigation.ca4773c5ff"),
  people: t("navigation.47ca7ee1c5"),
  applications: t("navigation.28f6b6e645"),
  settings: t("navigation.68ea5dd4d7"),
};
export function navigate(): void {
  const requested = location.hash.slice(1),
    page = titles[requested] ? requested : "groups";
  get("page-title").textContent = titles[page];
  for (const name of ["people", "applications", "settings"])
    get(name + "-page").hidden = name !== page;
  const conversation = ["groups", "direct", "boards"].includes(page);
  get("conversations").hidden = !conversation;
  if (conversation) setSection(page as Section);
  if (page === "settings") void loadServices();
  if (page === "people") void loadPeople();
  if (page === "applications") void loadApplications();
  document.querySelectorAll<HTMLElement>("[data-page]").forEach((node) => {
    node.setAttribute(
      "aria-current",
      node.dataset.page === page ? "page" : "false",
    );
  });
}
export function initNavigation(): void {
  document.querySelectorAll<HTMLElement>("[data-page]").forEach((node) => {
    node.onclick = () => {
      location.hash = node.dataset.page!;
    };
  });
  window.addEventListener("hashchange", navigate);
  navigate();
}
