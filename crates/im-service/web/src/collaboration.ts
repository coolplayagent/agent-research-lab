import { api, query } from "./api.js";
import { button, el, get, presence } from "./dom.js";
import { t, label } from "./i18n.js";
import { display, people } from "./people.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { messageSnapshot, locateMessage } from "./messages.js";
import { renderCrystal } from "./collaboration-graph.js";
import { revealResearch } from "./workspace.js";
import type { GroupView, Member, Page, Person } from "./types.js";
import type { ServiceView } from "./service-types.js";
let view: GroupView | null = null,
  selected = "",
  sequence = 0,
  epoch = 0;
let members: Member[] = [],
  after: string | null = null;
let bindings: Record<string, string> | null = null;
let bindingRequest = 0;
async function refreshBindings(): Promise<void> {
  if (!view) return;
  const context = epoch, request = ++bindingRequest;
  bindings = null;
  if (selected) detail(selected);
  try {
    const result = await api<ServiceView>("/api/im/services");
    if (context !== epoch || request !== bindingRequest) return;
    bindings = result.configuration.bindings;
    if (selected) detail(selected);
  } catch {
    /* Availability is unknown after a failed refresh. */
  }
}
let timer: ReturnType<typeof setTimeout> | undefined;
export function selectCollaboration(value: GroupView | null): void {
  sequence++;
  epoch++;
  clearTimeout(timer);
  view = value;
  selected = "";
  bindings = null;
  members = value?.members.members || [];
  after = value?.members.next_after || null;
  get("collaboration-detail").replaceChildren();
  render();
  if (value) void refreshBindings();
}
export function refreshCollaboration(): void {
  if (!view) return;
  clearTimeout(timer);
  timer = setTimeout(() => void loadMembers(false), 150);
}
async function loadMembers(more: boolean): Promise<void> {
  if (!view) return;
  const request = ++sequence,
    id = view.group.id;
  try {
    let cursor = more ? after || "" : "";
    const target = more ? 0 : members.length;
    let loaded: Member[] = [];
    do {
      const page = await api<GroupView>(
        "/api/crystal/view?" + query({ group_id: id, after: cursor }),
      );
      if (request !== sequence) return;
      view = page;
      loaded.push(...page.members.members);
      const next = page.members.next_after || "";
      if (next === cursor) {
        cursor = "";
        break;
      }
      cursor = next;
    } while (!more && cursor && loaded.length < target);
    members = more ? [...members, ...loaded] : loaded;
    after = cursor || null;
    render();
  } catch (error) {
    if (request === sequence)
      get("collaboration-status").textContent = String(error);
  }
}
function detail(id: string): void {
  selected = id;
  const member = members.find((m) => m.person_id === id);
  if (!member) {
    get("collaboration-detail").replaceChildren();
    return;
  }
  const panel = get("collaboration-detail");
  panel.replaceChildren(
    el("h3", display(id)),
    el("p", presence(member.presence)),
  );
  if (id !== "operator") {
    panel.append(
      el(
        "p",
        bindings === null
          ? t("workspace.bindingUnknown")
          : bindings[id]
            ? t("workspace.bound", { adapter: bindings[id] })
            : t("workspace.unbound"),
      ),
    );
    const link = el("a", t("workspace.configureExecution"));
    link.href = "#settings/bindings";
    panel.append(link);
  }
  for (const goal of goalSnapshot()) {
    for (const work of goal.work.filter((work) => work.person_id === id)) {
      panel.append(
        button(goal.input.title, () => focusGoal(goal.input.id)),
        el("p", `${label("WorkState", work.state)} · ${work.instruction}`),
      );
    }
  }
  const message = messageSnapshot()
    .filter((m) => m.sender_id === id)
    .at(-1);
  if (message)
    panel.append(
      button(t("workspace.locateMessage"), () => {
        get("workspace").classList.remove("research-expanded", "research-open");
        locateMessage(message.sequence);
      }),
    );
}
function render(): void {
  const host = get("collaboration-graph"),
    list = get("research-members");
  host.replaceChildren();
  list.replaceChildren();
  get("more-collaboration").hidden = !after;
  get("collaboration-status").textContent = view
    ? t("workspace.loadedMembers", {
        count: members.length,
        total: view.group.member_count,
      })
    : t("workspace.selectGroup");
  if (!view) return;
  renderCrystal(host, view.group, members, messageSnapshot(), goalSnapshot(), detail);
  for (const member of members) {
    const row = el("div", undefined, "member");
    row.append(
      button(display(member.person_id), () => {
        revealResearch("collaboration");
        detail(member.person_id);
      }),
      el("small", presence(member.presence)),
    );
    list.append(row);
  }
  if (after)
    list.append(button(t("html.ae4ed171f5"), () => void loadMembers(true)));
  if (selected) detail(selected);
}
export function initCollaboration(): void {
  get("more-collaboration").onclick = () => void loadMembers(true);
  get("refresh-collaboration").onclick = () => {
    void loadMembers(false);
    void refreshBindings();
  };
  document.addEventListener("workspace-services", () => void refreshBindings());
  let width = 0;
  new ResizeObserver(([entry]) => {
    const next = Math.round(entry.contentRect.width);
    if (next && Math.abs(next - width) > 10) { width = next; render(); }
  }).observe(get("collaboration-graph"));
  document.addEventListener("workspace-messages", render);
  document.addEventListener("workspace-goals", render);
  void (async () => {
    let cursor = "",
      count = 0;
    do {
      const page = await api<Page<Person>>(
        "/api/im/people?" + query({ after: cursor }),
      );
      for (const person of page.people) people.set(person.id, person);
      const next = page.next_after || "";
      if (next === cursor) break;
      cursor = next;
      count += page.people.length;
    } while (cursor && count < 10000);
    render();
  })().catch(() => {});
}
