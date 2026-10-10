import { workspaceState } from "./state.js";
import { short, $, time, el } from "./ui.js";
import { closePerson } from "./people-directory.js";
import { closeSession } from "./sessions.js";
import { dismissProfile } from "./preferences.js";
import { name } from "./identity.js";
export function roomName(id?: any): any {
  const members = workspaceState.data.jobs.filter(
      (j?: any) => j.cohort_id === id,
    ),
    topics = [...new Set<any>(members.flatMap((j?: any) => j.topics || []))],
    titles = {
      sdlc: "可验证自动交付",
      memory: "跨会话记忆与恢复",
      collaboration: "多 agent 协作边界",
      computer: "操作与沙箱恢复",
    };
  return topics.length === 1
    ? titles[topics[0]] || topics[0]
    : members[0]?.team || short(id);
}
export function showSettingsCategory(category?: any): any {
  if (
    !["people", "executors", "evolution", "observation", "service"].includes(
      category,
    )
  )
    category = workspaceState.settingsCategory;
  if (
    category !== "people" &&
    typeof closePerson === "function" &&
    $("person-window").classList.contains("embedded")
  )
    closePerson();
  workspaceState.settingsCategory = category;
  for (const id of [
    "people",
    "executors",
    "evolution",
    "observation",
    "service",
  ]) {
    $("settings-" + id).hidden = id !== category;
    if (id === category)
      $("settings-nav-" + id).setAttribute("aria-current", "page");
    else $("settings-nav-" + id).removeAttribute("aria-current");
  }
}
export function showPage(route?: any, focus: any = false): any {
  let [page, category] = route.split("/");
  if (!["collaboration", "messages", "settings"].includes(page))
    page = "collaboration";
  if (page === "settings") showSettingsCategory(category);
  else if (
    typeof closePerson === "function" &&
    $("person-window").classList.contains("embedded")
  )
    closePerson();
  for (const id of ["collaboration", "messages", "settings"]) {
    $(id + "-page").hidden = id !== page;
    if (id === page) $("nav-" + id).setAttribute("aria-current", "page");
    else $("nav-" + id).removeAttribute("aria-current");
  }
  if (page !== "collaboration") {
    if ($("session-window").open) closeSession();
    dismissProfile();
    if (focus) $(page + "-title").focus({ preventScroll: true });
  }
}
export function renderSystemMessages(): any {
  const current = new Map<any, any>(),
    control = workspaceState.data?.controller || {};
  for (const [key, text] of workspaceState.serviceAlerts)
    current.set(key, text);
  if (workspaceState.snapshotError)
    current.set("snapshot-request", workspaceState.snapshotError);
  if (workspaceState.data?.error)
    current.set("snapshot", String(workspaceState.data.error));
  for (const j of workspaceState.data?.jobs || []) {
    if (j.postprocessing?.persona_memory_error)
      current.set(
        `persona-memory:${j.run_id}`,
        `${name(j)} 的记忆写回尚未确认：${j.postprocessing.persona_memory_error}`,
      );
  }
  if (workspaceState.data && !workspaceState.connected)
    current.set(
      "connection",
      "实时连接暂时中断，正在自动重连。页面会继续尝试获取最新快照。",
    );
  if (workspaceState.data && !control.live)
    current.set("controller", "研究控制器离线或尚未上报状态。");
  else if (control.stale)
    current.set(
      "heartbeat",
      `控制器心跳已延迟 ${control.heartbeat_age_seconds} 秒，正在持续观察。`,
    );
  if (control.seed?.error)
    current.set(
      "seed",
      `新研究准入受阻：${control.seed.error}${control.seed.next_attempt_at ? " · 下次检查 " + time(control.seed.next_attempt_at) : ""}`,
    );
  for (const warning of workspaceState.data?.warnings || [])
    current.set("warning:" + warning, warning);
  const visibleCurrent = new Map<any, any>([...current].slice(0, 100));
  const now = Date.now();
  for (const [key, entry] of workspaceState.systemEntries) {
    if (current.has(key) && !visibleCurrent.has(key)) {
      workspaceState.systemEntries.delete(key);
      continue;
    }
    if (entry.active && !current.has(key)) {
      entry.active = false;
      entry.changedAt = now;
    }
  }
  for (const [key, text] of visibleCurrent) {
    const old = workspaceState.systemEntries.get(key);
    if (!old || !old.active)
      workspaceState.systemEntries.set(key, {
        text,
        active: true,
        changedAt: now,
      });
    else old.text = text;
  }
  const entries = [...workspaceState.systemEntries].sort(
    ([, a]: any, [, b]: any) =>
      Number(b.active) - Number(a.active) || b.changedAt - a.changedAt,
  );
  for (const [key] of entries.slice(100))
    workspaceState.systemEntries.delete(key);
  const active = current.size;
  $("system-unread").hidden = !active;
  $("system-unread").textContent = active;
  $("system-message-count").textContent = active
    ? `${active} 条待关注${active > 100 ? " · 显示前 100 条" : ""}`
    : "当前无告警";
  const key = JSON.stringify(entries.slice(0, 100));
  if (workspaceState.systemKey === key) return;
  workspaceState.systemKey = key;
  const root = $("system-messages");
  root.replaceChildren();
  for (const [, entry] of entries.slice(0, 100)) {
    const card = el(
        "article",
        undefined,
        `system-message ${entry.active ? "active" : "resolved"}`,
      ),
      heading = el("div", undefined, "system-message-heading");
    heading.append(
      el("strong", entry.active ? "需要关注" : "已恢复"),
      el("time", new Date(entry.changedAt).toLocaleString("zh-CN")),
    );
    card.append(heading, el("p", entry.text));
    root.append(card);
  }
  if (!entries.length) root.append(el("div", "当前没有系统告警。", "empty"));
}
