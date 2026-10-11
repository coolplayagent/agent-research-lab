import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice, value } from "./dom.js";
import type { Directory, Group } from "./types.js";
import { searchKeys } from "./experience.js";
export type Section = "groups" | "direct" | "boards";
let section: Section = "groups",
  next: string | null = null,
  rows: Group[] = [],
  selected = "",
  generation = 0;
let choose: (group: Group) => void;
export function setSection(value: Section): void {
  section = value;
  render();
}
export function setSelected(id: string): void {
  selected = id;
  render();
}
function belongs(group: Group): boolean {
  return section === "boards"
    ? group.kind === GroupKind.Board
    : section === "direct"
      ? group.private || group.kind === GroupKind.Temporary
      : group.kind === GroupKind.Conversation && !group.private;
}
function render(): void {
  const root = get("group-list");
  root.replaceChildren();
  const visible = rows.filter(belongs);
  get("directory-count").textContent = t("ux.directoryCount", { count: visible.length });
  for (const group of visible) {
    const item = button("", () => choose(group));
    item.className = "group-item" + (group.id === selected ? " selected" : "");
    item.setAttribute("aria-current", group.id === selected ? "true" : "false");
    item.append(
      el("strong", `${group.pinned ? "★ " : ""}${group.title}`),
      el(
        "small",
        t("directory.c04a113aca", {
          p0: group.archived ? t("directory.bafe86d881") : "",
          p1: group.member_count,
        }),
      ),
    );
    root.append(item);
  }
  if (!root.childElementCount) {
    const empty = el("div", undefined, "directory-empty");
    empty.append(el("strong", value("group-search") ? t("ux.noResults") : t("directory.49ff4c1834")));
    if (value("group-search")) {
      empty.append(el("p", t("ux.noResultsNote")), button(t("ux.clearSearch"), () => {
        get<HTMLInputElement>("group-search").value = "";
        return loadGroups();
      }));
    } else empty.append(button(t("ux.createConversation"), () => get("new-conversation").click()));
    root.append(empty);
  }
}
export function metrics(data: Record<string, unknown>): void {
  const root = get("metrics");
  root.replaceChildren();
  for (const [label, key] of [
    ["Agent", "actors"],
    [t("directory.0a29c2ff50"), "groups"],
    [t("directory.4cc404a451"), "stored_messages"],
    [t("directory.a1b56411cc"), "subscriptions"],
    [t("directory.e1fa4a08fc"), "queue_depth"],
    [t("directory.56eb5b282c"), "replayed"],
    [t("directory.1e55edfca7"), "rejected"],
  ])
    root.append(el("dt", label), el("dd", String(data[key] ?? "—")));
  root.append(
    el("dt", t("directory.f0f4fa0c46")),
    el(
      "dd",
      data.healthy ? t("directory.296de0e31f") : t("directory.20f8e5ca6d"),
    ),
  );
}
export async function loadGroups(append = false): Promise<void> {
  const request = ++generation;
  get("group-list").setAttribute("aria-busy", "true");
  if (!rows.length) get("directory-count").textContent = t("ux.loading");
  try {
    const result = await api<Directory>(
      "/api/crystal/view?" +
        query({
          after: append ? next || "" : "",
          query: value("group-search"),
        }),
    );
    if (generation !== request) return;
    rows = append
      ? [...rows, ...result.directory.groups]
      : result.directory.groups;
    next = result.directory.next_after;
    get("more-groups").hidden = !next;
    metrics(result.metrics);
    render();
  } catch (error) {
    if (generation === request) { notice(error); get("directory-count").textContent = t("ux.searchFailed"); }
  } finally {
    if (generation === request) get("group-list").removeAttribute("aria-busy");
  }
}
export function initDirectory(select: (group: Group) => void): void {
  choose = select;
  searchKeys(get<HTMLInputElement>("group-search"), () => void loadGroups());
  get("search-groups").onclick = () => void loadGroups();
  get("more-groups").onclick = () => void loadGroups(true);
}
