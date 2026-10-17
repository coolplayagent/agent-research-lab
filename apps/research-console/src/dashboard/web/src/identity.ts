import { ExecutionState, Presence } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { workspaceState } from "./state.js";
import { el, svg, $ } from "./ui.js";
import {
  savePreferences,
  applySidebarLayout,
  dismissProfile,
  setChatCollapsed,
} from "./preferences.js";
import { details } from "./profile.js";
import { render } from "./observer.js";
import { openSession } from "./sessions.js";
export function role(j?: any): any {
  return j.id.startsWith("synthesis-") ? "synthesis" : j.role;
}
export function agentPresence(j?: any): any {
  const member = workspaceState.data?.boards
    .flatMap((b?: any) => b.members || [])
    .find((m?: any) => m.run_id === j.run_id);
  const offline =
    j.state !== ExecutionState.Running ||
    !workspaceState.data?.controller?.live ||
    workspaceState.data?.controller?.stale;
  return {
    state: offline
      ? Presence.Offline
      : member?.presence?.state || Presence.Online,
    pending: member?.presence?.pending || 0,
  };
}
export function presenceLabel(j?: any): string {
  return label("Presence", agentPresence(j).state);
}
export function name(j?: any): any {
  return j.profile?.display_name || j.id;
}
export function roleLabel(j?: any): any {
  return (
    {
      research: t("identity.4ff0f1dda8"),
      review: t("identity.07bf038d63"),
      implement: t("identity.11dd7ef8a8"),
      synthesis: t("identity.4a0d4edef9"),
    }[role(j)] || j.role
  );
}
export function avatar(j?: any): any {
  const face = el("span", undefined, `avatar ${role(j)}`),
    picture = svg("svg", { viewBox: "0 0 40 40", "aria-hidden": "true" });
  picture.append(
    svg("path", {
      d: "M10 35c0-9 20-9 20 0",
      fill: "currentColor",
      opacity: ".24",
    }),
    svg("rect", {
      x: 9,
      y: 7,
      width: 22,
      height: 22,
      rx: 9,
      fill: "currentColor",
      opacity: ".18",
    }),
    svg("circle", { cx: 16, cy: 17, r: 2, fill: "currentColor" }),
    svg("circle", { cx: 24, cy: 17, r: 2, fill: "currentColor" }),
    svg("path", {
      d: "M16 23q4 3 8 0",
      fill: "none",
      stroke: "currentColor",
      "stroke-width": 1.5,
    }),
  );
  face.append(picture);
  return face;
}
export function handle(j?: any): any {
  return j.profile?.handle || j.id;
}
export function mention(j?: any): any {
  const button = el("button", `@${name(j)}`, "mention");
  button.title = `@${handle(j)} · ${j.id}`;
  button.addEventListener("click", () => openProfile(j.id));
  return button;
}
export function openProfile(id?: any): any {
  if (!workspaceState.data?.jobs.some((j?: any) => j.id === id)) return;
  workspaceState.inspected = id;
  workspaceState.preferences.rightCollapsed = false;
  savePreferences();
  document.body.classList.add("profile-open");
  applySidebarLayout();
  details(workspaceState.data.jobs.find((j?: any) => j.id === id));
  $("close-profile").focus({ preventScroll: true });
}
export function maximize(panel?: any): any {
  const next = workspaceState.focusPanel === panel ? "" : panel;
  if (next) dismissProfile();
  if (!workspaceState.focusPanel && next)
    workspaceState.focusScroll = $("timeline").scrollTop;
  workspaceState.focusPanel = next;
  if (next === "chat") setChatCollapsed(false);
  applySidebarLayout();
  document.body.classList.toggle("focus-graph", next === "graph");
  document.body.classList.toggle("focus-chat", next === "chat");
  for (const name of ["graph", "chat"]) {
    const button = $("maximize-" + name),
      active = next === name;
    button.textContent = active
      ? t("identity.01831fe8e3")
      : t("identity.5a2a22c0d8");
    button.setAttribute("aria-pressed", String(active));
    button.setAttribute(
      "aria-label",
      `${active ? t("identity.ddde089462") : t("identity.80f8fbcfa0")}${name === "graph" ? t("identity.ca546f3abe") : t("identity.d155c1aac5")}`,
    );
  }
  if (!next && workspaceState.focusScroll !== null) {
    const scroll = workspaceState.focusScroll;
    requestAnimationFrame(() => {
      $("timeline").scrollTop = scroll;
    });
    workspaceState.focusScroll = null;
  }
}
export async function loadProfile(j?: any): Promise<any> {
  if (workspaceState.profiles.has(j.run_id)) return;
  workspaceState.profiles.set(j.run_id, null);
  try {
    const r = await fetch(`/api/profile/${encodeURIComponent(j.run_id)}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!r.ok) throw new Error("profile");
    workspaceState.profiles.set(j.run_id, await r.json());
  } catch (_) {
    workspaceState.profiles.set(j.run_id, {
      error: t("identity.b16790b66b"),
    });
  }
  if ((workspaceState.inspected || workspaceState.selected) === j.id)
    details(j);
}
export function fold(key?: any, title?: any, cls?: any): any {
  const block = el("details", undefined, cls);
  block.dataset.fold = key;
  block.open = workspaceState.opened.has(key);
  block.addEventListener("toggle", () => {
    if (!block.isConnected) return;
    if (block.open) workspaceState.opened.add(key);
    else workspaceState.opened.delete(key);
  });
  block.append(el("summary", title));
  return block;
}
export function readable(text?: any, key?: any, cls: any = "public-text"): any {
  const root = el("div", undefined, cls);
  if (text.length <= 900) root.append(el("div", text));
  else {
    root.append(el("div", text.slice(0, 600) + "…"));
    const more = fold(key, t("identity.fb654bf587"), "message-more");
    more.append(el("div", text));
    root.append(more);
  }
  return root;
}
export function cohortJobs(): any {
  return (workspaceState.data?.jobs || []).filter(
    (j?: any) => !workspaceState.room || j.cohort_id === workspaceState.room,
  );
}
export function jobs(): any {
  const q = $("search").value.toLowerCase().replace(/^@/, "");
  return cohortJobs().filter(
    (j?: any) =>
      (!$("state").value || j.state === $("state").value) &&
      [
        j.id,
        handle(j),
        name(j),
        j.model,
        j.role,
        j.repository,
        ...(j.topics || []),
      ]
        .join(" ")
        .toLowerCase()
        .includes(q),
  );
}
export function session(j?: any): any {
  return (
    (workspaceState.data.sessions || []).find(
      (s?: any) => s.run_id === j.run_id,
    ) ||
    (workspaceState.extraSession?.run_id === j.run_id
      ? workspaceState.extraSession
      : null)
  );
}
export function choose(id?: any, showSession: any = false): any {
  workspaceState.selected = id;
  workspaceState.inspected = id;
  render();
  if (showSession) openSession(id);
}
