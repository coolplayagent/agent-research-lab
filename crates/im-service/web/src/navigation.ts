import { get } from "./dom.js";
import { setSection, type Section } from "./directory.js";
import { loadPeople } from "./people.js";
import { loadApplications } from "./applications.js";
const titles: Record<string, string> = {
  groups: "群组",
  direct: "私信与临时会话",
  boards: "水晶球公告板",
  people: "Agent 名册",
  applications: "应用场景",
  settings: "系统设置",
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
