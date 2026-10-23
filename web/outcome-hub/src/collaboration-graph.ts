import { avatarSvg } from "./avatar.js";
import { memberRunningWork } from "./goal-selection.js";
import { t, label } from "./i18n.js";
import { display } from "./people.js";
import { el, presence } from "./dom.js";
import { replyEdges } from "./collaboration-model.js";
import type { Group, Member, Message } from "./types.js";
import type { Goal } from "./goal-types.js";
const ns = "http://www.w3.org/2000/svg";
function compactName(name: string, limit: number): string {
  const letters = Array.from(name);
  if (letters.length <= limit) return name;
  const start = Math.floor((limit - 1) / 2);
  return letters.slice(0, start).join("") + "…" + letters.slice(-(limit - start - 1)).join("");
}
function svg<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}, text?: string): SVGElementTagNameMap[K] {
  const node = document.createElementNS(ns, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
  if (text !== undefined) node.textContent = text;
  return node;
}
export function renderCrystal(host: HTMLElement, group: Group, members: Member[], messages: Message[], goals: Goal[], select: (id: string) => void, pulses: Set<string>, openConversation: () => void): void {
  const now = Date.now(), scopedGoals = goals.filter(goal => goal.input.group_id === group.id);
  const wide = host.clientWidth > 550;
  const width = wide ? 800 : 360, height = wide ? 490 : 440;
  const cx = width / 2, cy = height / 2, radius = wide ? 82 : 57;
  const shown = members.slice(0, 8);
  if (members.length > shown.length) host.append(el("p", t("workspace.graphLimit", { count: shown.length })));
  const canvas = svg("svg", { viewBox: `0 0 ${width} ${height}`, role: "group", "aria-label": t("workspace.crystal"), class: "crystal-canvas" });
  const defs = svg("defs");
  const gradient = svg("radialGradient", { id: "crystal-fill", cx: "32%", cy: "22%", r: "80%" });
  for (const [offset, color] of [["0%", "#ffffff"], ["44%", "#dcf9f3"], ["82%", "#a6ded9"], ["100%", "#d6eef2"]]) gradient.append(svg("stop", { offset, "stop-color": color }));
  defs.append(gradient);
  for (const [id, color] of [["crystal-publish", "#3faaa0"], ["crystal-reply", "#6b9ebd"]]) {
    const marker = svg("marker", { id, viewBox: "0 0 10 10", refX: 9, refY: 5, markerWidth: 5, markerHeight: 5, orient: "auto" });
    marker.append(svg("path", { d: "M0 0 L10 5 L0 10 z", fill: color }));
    defs.append(marker);
  }
  canvas.append(defs, svg("ellipse", { cx, cy, rx: wide ? 287 : 138, ry: wide ? 179 : 167, class: "crystal-orbit" }), svg("circle", { cx, cy, r: radius + 18, class: "crystal-halo" }));
  const positions = new Map<string, { x: number; y: number }>();
  shown.forEach((member, index) => {
    const angle = -Math.PI / 2 + index * Math.PI * 2 / shown.length;
    const x = cx + Math.cos(angle) * (wide ? 282 : 121), y = cy + Math.sin(angle) * (wide ? 177 : 166);
    positions.set(member.person_id, { x, y });
    const endX = cx + Math.cos(angle) * (radius + 5), endY = cy + Math.sin(angle) * (radius + 5);
    const startX = x - Math.cos(angle) * (wide ? 75 : 40), startY = y - Math.sin(angle) * 29;
    const path = `M${startX},${startY} L${endX},${endY}`;
    canvas.append(svg("path", { d: path, class: "collaboration-edge" }));
    if (pulses.has(member.person_id)) canvas.append(svg("path", { d: path, class: "crystal-traffic" }));
    if (messages.some((message) => message.sender_id === member.person_id && (!message.event || message.event.person_id === member.person_id)))
      canvas.append(svg("path", { d: path, class: "crystal-publish", "marker-end": "url(#crystal-publish)" }));
  });
  for (const edge of replyEdges(messages)) {
    const from = positions.get(edge.from), to = positions.get(edge.to);
    if (!from || !to) continue;
    const path = svg("path", { d: `M${from.x},${from.y} Q${cx},${cy - radius - 20} ${to.x},${to.y}`, class: "reply-edge" + (pulses.has(edge.from) ? " crystal-traffic" : ""), "marker-end": "url(#crystal-reply)" });
    path.append(svg("title", {}, t("workspace.replyEdge", { from: display(edge.from), to: display(edge.to) })));
    canvas.append(path);
  }
  const orb = svg("g", { class: "crystal-center" + (pulses.size ? " receiving" : ""), role: "button", tabindex: 0, "aria-label": t("workspace.openConversation") });
  orb.onclick = openConversation;
  orb.onkeydown = (event) => { if (["Enter", " "].includes(event.key)) { event.preventDefault(); openConversation(); } };
  orb.append(svg("circle", { cx, cy, r: radius, fill: "url(#crystal-fill)" }), svg("ellipse", { cx: cx - radius * .2, cy: cy - radius * .4, rx: radius * .46, ry: radius * .23, fill: "#fff", opacity: .6, transform: `rotate(-25 ${cx} ${cy})` }), svg("text", { x: cx, y: cy - 6, class: "crystal-title" }, t("workspace.crystal")), svg("text", { x: cx, y: cy + 15, class: "crystal-sub" }, t("workspace.crystalMessages", { count: messages.filter((m) => !m.event || m.sender_id === m.event.person_id).length })));
  orb.append(svg("title", {}, group.title));
  canvas.append(orb);
  shown.forEach((member) => {
    const { x, y } = positions.get(member.person_id)!;
    const w = wide ? 166 : 112;
    const work = memberRunningWork(scopedGoals, member.person_id, now);
    const node = svg("g", { class: `agent-node${work ? " working" : ""}${pulses.has(member.person_id) ? " speaking" : ""}`, transform: `translate(${x - w / 2},${y - 29})`, role: "button", tabindex: 0, "aria-label": `@${display(member.person_id)} · ${presence(member.presence, member.archived)}` });
    node.append(svg("rect", { width: w, height: 58, rx: 10 }), svg("circle", { cx: 31, cy: 43, r: 3, class: "agent-presence" }), svg("text", { x: 45, y: 25, class: "agent-name" }, `@${compactName(display(member.person_id), wide ? 12 : 4)}`), svg("text", { x: 45, y: 43, class: "agent-activity" }, presence(member.presence, member.archived)), svg("title", {}, display(member.person_id)));
    const face = avatarSvg(member.person_id);
    face.setAttribute("x", "7"); face.setAttribute("y", "12"); face.setAttribute("width", "32"); face.setAttribute("height", "32");
    node.append(face);
    node.onclick = () => select(member.person_id);
    node.onkeydown = (event) => { if (["Enter", " "].includes(event.key)) { event.preventDefault(); select(member.person_id); } };
    canvas.append(node);
  });
  host.append(canvas);
}
