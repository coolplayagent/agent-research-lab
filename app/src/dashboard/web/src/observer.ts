import { ExecutionState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { workspaceState } from "./state.js";
import {
  jobs,
  cohortJobs,
  name,
  roleLabel,
  presenceLabel,
  agentPresence,
  avatar,
  choose,
} from "./identity.js";
import { rooms, privateChats, timeline } from "./channels.js";
import { $, el } from "./ui.js";
import { roomName, renderSystemMessages } from "./navigation.js";
import { openPerson } from "./people-profile.js";
import { renderPeopleSidebar, syncPeople } from "./people-directory.js";
import { graph } from "./graph.js";
import { details } from "./profile.js";
import { renderSession, fetchSession } from "./sessions.js";
import { observeLineage } from "./lineage.js";
export function render(): any {
  if (!workspaceState.data) return;
  const list = jobs(),
    all = cohortJobs();
  if (!list.some((j?: any) => j.id === workspaceState.selected))
    workspaceState.selected = list[0]?.id || "";
  rooms();
  privateChats();
  $("room-title").textContent = workspaceState.room
    ? "# " + roomName(workspaceState.room)
    : t("observer.f6ec211770");
  $("room-subtitle").textContent = t("observer.c337f088de", {
    p0: all.length,
    p1: workspaceState.data.max_agents || 8,
  });
  const count = (states?: any) =>
    all.filter((j?: any) => states.includes(j.state)).length;
  $("running").textContent = count([ExecutionState.Running]);
  $("pending").textContent = count([
    ExecutionState.Ready,
    ExecutionState.WaitingDependencies,
    ExecutionState.RetryWait,
  ]);
  $("succeeded").textContent = count([ExecutionState.Succeeded]);
  $("blocked").textContent = count([
    ExecutionState.Blocked,
    ExecutionState.Failed,
    ExecutionState.NeedsReconciliation,
    ExecutionState.Unknown,
  ]);
  $("history-count").textContent = t("observer.f5a287c86f", {
    p0: workspaceState.data.jobs.length,
    p1: workspaceState.data.summary?.blocked || 0,
  });
  const control = workspaceState.data.controller || {},
    phases = {
      starting: t("observer.33439d2631"),
      recovering: t("observer.24f458b7f5"),
      refreshing: t("observer.c6395daadb"),
      idle: t("observer.1a474c3207"),
      running: t("observer.8408d4fa09"),
      stopped: t("observer.f006455e3b"),
      unknown: t("observer.2f040192b1"),
    };
  $("controller-status").textContent = control.live
    ? phases[control.phase] || control.phase
    : t("observer.b09fa4a571");
  $("setting-follow").checked = workspaceState.following;
  $("setting-motion").checked = workspaceState.preferences.motion !== false;
  const info = $("service-settings-info");
  info.replaceChildren();
  for (const [label, value] of [
    [t("observer.e8a4f7c09d"), $("controller-status").textContent],
    [
      t("observer.652a6f861b"),
      t("observer.c0ca38f586", { p0: workspaceState.data.max_agents || 8 }),
    ],
    [
      t("observer.75db758dea"),
      t("observer.8eaa29c22a", { p0: workspaceState.data.jobs.length }),
    ],
    [t("observer.975b35f9c2"), "SuperPOD"],
  ])
    info.append(el("dt", label), el("dd", value));
  const rows = $("tasks");
  const scroll = rows.scrollTop;
  rows.replaceChildren();
  const listedPeople = new Set<any>();
  list.forEach((j?: any) => {
    const personId = j.profile?.person_id || j.id;
    if (listedPeople.has(personId)) return;
    listedPeople.add(personId);
    const b = el(
        "button",
        undefined,
        `task-row ${j.id === workspaceState.selected ? "selected" : ""}`,
      ),
      text = el("span", name(j), "task-name");
    text.append(
      el("small", `${roleLabel(j)} · ${j.model} · ${presenceLabel(j)}`),
    );
    const dot = el("span", undefined, `presence ${agentPresence(j).state}`);
    dot.title = presenceLabel(j);
    b.append(avatar(j), text, dot);
    b.title = j.id;
    b.addEventListener("click", () =>
      j.profile?.person_id
        ? openPerson(j.profile.person_id)
        : choose(j.id, true),
    );
    rows.append(b);
  });
  rows.scrollTop = scroll;
  $("task-count").textContent = listedPeople.size;
  if (typeof renderPeopleSidebar === "function") renderPeopleSidebar(list);
  {
    graph(list);
    details(
      workspaceState.data.jobs.find(
        (j?: any) =>
          j.id === (workspaceState.inspected || workspaceState.selected),
      ),
    );
    timeline(list);
  }
  renderSession();
  connection();
  observeLineage(valueLineageRevision());
}
export function valueLineageRevision(): any {
  return workspaceState.data?.lineage_revision ?? null;
}
export function accept(value?: any): any {
  workspaceState.snapshotError = "";
  value.jobs = value.jobs || [];
  value.boards = value.boards || [];
  value.sessions = value.sessions || [];
  if (typeof syncPeople === "function") syncPeople(value.roster);
  const knownRuns = new Set<any>(value.jobs.map((j?: any) => j.run_id));
  for (const id of workspaceState.profiles.keys())
    if (!knownRuns.has(id)) workspaceState.profiles.delete(id);
  let sessionChanged = false;
  for (const s of value.sessions) {
    const old = workspaceState.lastBytes.get(s.run_id);
    if (old !== undefined && old !== s.bytes)
      workspaceState.pulses.set(s.run_id, Date.now() + 1600);
    if (old !== s.bytes && workspaceState.historyState?.run === s.run_id)
      sessionChanged = true;
    workspaceState.lastBytes.set(s.run_id, s.bytes);
  }
  if (workspaceState.data) {
    const known = new Set<any>(
      workspaceState.data.boards.flatMap((b?: any) =>
        (b.retained_messages || []).map((m?: any) => m.message.id),
      ),
    );
    for (const b of value.boards)
      for (const m of b.retained_messages || [])
        if (!known.has(m.message.id))
          workspaceState.publishPulses.set(
            m.message.task_id,
            Date.now() + 2000,
          );
  }
  workspaceState.data = value;
  if (
    !workspaceState.data.jobs.some(
      (j?: any) => j.id === workspaceState.inspected,
    )
  )
    workspaceState.inspected = "";
  if (workspaceState.following) {
    const active = workspaceState.data.jobs.find(
        (j?: any) => j.state === ExecutionState.Running && j.cohort_id,
      ),
      seed = workspaceState.data.jobs.find(
        (j?: any) =>
          (workspaceState.data.controller?.seed?.queued || []).includes(j.id) &&
          j.cohort_id,
      );
    workspaceState.room =
      active?.cohort_id ||
      seed?.cohort_id ||
      workspaceState.data.jobs.find((j?: any) => j.cohort_id)?.cohort_id ||
      "";
  }
  render();
  if (
    sessionChanged &&
    workspaceState.historyState?.following &&
    $("session-window").open
  )
    fetchSession();
}
export function connection(): any {
  renderSystemMessages();
  const age = workspaceState.data?.sampled_at
    ? Math.max(
        0,
        Math.floor(Date.now() / 1000 - workspaceState.data.sampled_at),
      )
    : null;
  const healthy =
    workspaceState.connected &&
    !!workspaceState.data &&
    !workspaceState.data.error;
  $("connection-dot").classList.toggle("live", healthy);
  $("connection").textContent = healthy
    ? t("observer.a1b56411cc")
    : workspaceState.connected
      ? t("observer.8ad7b78fb1")
      : t("observer.7a58d3d9d3");
  $("sample-time").textContent =
    age === null
      ? t("observer.e740c32f21")
      : t("observer.1e0a0d0e4c", { p0: age });
}
export async function refresh(): Promise<any> {
  if (workspaceState.fetching) return;
  workspaceState.fetching = true;
  $("refresh").disabled = true;
  try {
    const r = await fetch("/api/snapshot", {
      cache: "no-store",
      signal: AbortSignal.timeout(8000),
    });
    if (!r.ok) throw new Error("snapshot");
    workspaceState.snapshotError = "";
    accept(await r.json());
  } catch (_) {
    workspaceState.snapshotError = t("observer.164e17de13");
    workspaceState.connected = false;
    connection();
  } finally {
    workspaceState.fetching = false;
    $("refresh").disabled = false;
  }
}
export function connect(): any {
  workspaceState.stream = new EventSource("/api/events");
  workspaceState.stream.addEventListener("snapshot", (e?: any) => {
    try {
      workspaceState.connected = true;
      accept(JSON.parse(e.data));
    } catch (_) {
      workspaceState.connected = false;
      connection();
    }
  });
  workspaceState.stream.onopen = () => {
    workspaceState.connected = true;
    connection();
  };
  workspaceState.stream.onerror = () => {
    workspaceState.connected = false;
    connection();
  };
}
