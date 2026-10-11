import { api, query } from "./api.js";
import { button, el, get, presence } from "./dom.js";
import { t, label } from "./i18n.js";
import { display, people } from "./people.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { messageSnapshot, locateMessage } from "./messages.js";
import { replyEdges } from "./collaboration-model.js";
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
let timer: ReturnType<typeof setTimeout> | undefined;
const ns = "http://www.w3.org/2000/svg";
function svg<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string>,
): SVGElementTagNameMap[K] {
  const node = document.createElementNS(ns, tag);
  for (const [key, value] of Object.entries(attrs))
    node.setAttribute(key, value);
  return node;
}
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
  if (value) {
    const request = epoch;
    void api<ServiceView>("/api/im/services")
      .then((result) => {
        if (request !== epoch) return;
        bindings = result.configuration.bindings;
        if (selected) detail(selected);
      })
      .catch(() => {
        /* Messaging and graph remain available without configuration. */
      });
  }
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
  const shown = members.slice(0, 32);
  if (members.length > shown.length)
    host.append(el("p", t("workspace.graphLimit", { count: shown.length })));
  const height = Math.max(330, Math.ceil(shown.length / 8) * 190);
  const canvas = svg("svg", {
    viewBox: `0 0 320 ${height}`,
    role: "group",
    "aria-label": t("workspace.crystal"),
  });
  const defs = svg("defs", {}),
    gradient = svg("radialGradient", {
      id: "collaboration-orb",
      cx: "30%",
      cy: "25%",
      r: "75%",
    });
  for (const [offset, color] of [
    ["0%", "#fff"],
    ["60%", "#d6f4e9"],
    ["100%", "#9ecfbd"],
  ])
    gradient.append(svg("stop", { offset, "stop-color": color }));
  defs.append(gradient);
  canvas.append(defs);
  const positions = new Map<string, { x: number; y: number }>();
  shown.forEach((member, index) => {
    const ring = Math.floor(index / 8),
      count = Math.min(8, shown.length - ring * 8);
    const angle = ((index % 8) / count) * Math.PI * 2 - Math.PI / 2;
    positions.set(member.person_id, {
      x: 160 + Math.cos(angle) * 112,
      y: 165 + ring * 190 + Math.sin(angle) * 112,
    });
  });
  for (const point of positions.values())
    canvas.append(
      svg("line", {
        x1: "160",
        y1: "165",
        x2: String(point.x),
        y2: String(point.y),
        class: "collaboration-edge",
      }),
    );
  for (const edge of replyEdges(messageSnapshot())) {
    const from = positions.get(edge.from),
      to = positions.get(edge.to);
    if (from && to) {
      const line = svg("path", {
        d: `M${from.x},${from.y} Q160,145 ${to.x},${to.y}`,
        class: "reply-edge",
      });
      const title = svg("title", {});
      title.textContent = t("workspace.replyEdge", {
        from: display(edge.from),
        to: display(edge.to),
      });
      line.append(title);
      canvas.append(line);
    }
  }
  canvas.append(
    svg("circle", {
      cx: "160",
      cy: "165",
      r: "37",
      fill: "url(#collaboration-orb)",
    }),
  );
  const title = svg("text", {
    x: "160",
    y: "170",
    "text-anchor": "middle",
    "font-size": "12",
  });
  title.textContent = view.group.title.slice(0, 9);
  canvas.append(title);
  shown.forEach((member) => {
    const point = positions.get(member.person_id)!;
    const node = svg("g", {
      class: "agent-node",
      role: "button",
      tabindex: "0",
      transform: `translate(${point.x},${point.y})`,
      "aria-label": `${display(member.person_id)} · ${presence(member.presence)}`,
    });
    node.append(svg("circle", { r: "24", fill: "url(#collaboration-orb)" }));
    const name = svg("text", { y: "4", "text-anchor": "middle" });
    name.textContent = display(member.person_id).slice(0, 5);
    const state = svg("text", { y: "39", "text-anchor": "middle" });
    state.textContent = presence(member.presence);
    node.append(name, state);
    const tasks = goalSnapshot()
      .flatMap((goal) => goal.work)
      .filter((work) => work.person_id === member.person_id).length;
    if (tasks) {
      const assignment = svg("text", { y: "54", "text-anchor": "middle" });
      assignment.textContent = t("workspace.assignments", { count: tasks });
      node.append(assignment);
    }
    node.onclick = () => detail(member.person_id);
    node.onkeydown = (event) => {
      if (["Enter", " "].includes(event.key)) {
        event.preventDefault();
        detail(member.person_id);
      }
    };
    canvas.append(node);
  });
  host.append(canvas);
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
  get("refresh-collaboration").onclick = () => void loadMembers(false);
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
