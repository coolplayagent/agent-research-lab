import { workspaceState } from "./state.js";
import { session, name, presenceLabel, avatar } from "./identity.js";
import { dismissProfile } from "./preferences.js";
import { $, time, el } from "./ui.js";
import { processBlock } from "./process.js";
export function openSession(id?: any): any {
  const j = workspaceState.data?.jobs.find((job?: any) => job.id === id);
  if (j) openSessionRecord(j);
}
export function openSessionRecord(j?: any): any {
  const id = j.id;
  workspaceState.sessionReturnFocus = document.activeElement;
  workspaceState.sessionAgent = id;
  workspaceState.historyState = {
    job: j,
    run: j.run_id,
    page: session(j),
    older: [],
    loading: false,
    following: true,
    error: "",
    key: "",
    request: 0,
  };
  dismissProfile();
  if (!$("session-window").open) $("session-window").showModal();
  renderSession(true);
  fetchSession();
}
export function closeSession(): any {
  $("session-window").close();
  workspaceState.sessionAgent = "";
  workspaceState.historyState = null;
  if (workspaceState.sessionReturnFocus?.isConnected)
    workspaceState.sessionReturnFocus.focus({ preventScroll: true });
  else $("open-profile").focus({ preventScroll: true });
}
export async function fetchSession(
  before: any = null,
  direction: any = "older",
): Promise<any> {
  const state = workspaceState.historyState;
  if (!state || state.loading) return;
  state.loading = true;
  state.error = "";
  const request = ++state.request;
  renderSession();
  try {
    const response = await fetch(
      `/api/session/${encodeURIComponent(state.run)}${before === null ? "" : `?before=${before}`}`,
      {
        cache: "no-store",
        signal: AbortSignal.timeout(5000),
      },
    );
    if (!response.ok) throw new Error("session");
    const page = await response.json();
    if (workspaceState.historyState !== state || request !== state.request)
      return;
    if (before !== null) {
      if (direction === "older") state.older.push(state.page?.page_end ?? null);
      if (direction === "newer") state.older.pop();
      state.following = direction === "newer" && !state.older.length;
    }
    if (before === null) state.following = true;
    state.page = page;
    workspaceState.extraSession =
      before === null ? page : workspaceState.extraSession;
    renderSession(before !== null);
  } catch (_) {
    if (workspaceState.historyState === state)
      state.error = "暂时无法读取会话，请稍后重试。";
  } finally {
    if (workspaceState.historyState === state) {
      state.loading = false;
      renderSession();
    }
  }
}
export function renderSession(resetScroll: any = false): any {
  const state = workspaceState.historyState,
    dialog = $("session-window");
  if (!state || !dialog.open) return;
  const j =
    workspaceState.data.jobs.find((job?: any) => job.run_id === state.run) ||
    state.job;
  if (!j || j.run_id !== state.run) {
    $("session-status").textContent =
      "这轮会话已结束或不在观察范围内，请重新选择数字人。";
    return;
  }
  $("session-title").textContent = name(j);
  $("session-subtitle").textContent =
    `${j.model} · ${presenceLabel(j)} · 第 ${j.attempt} 轮`;
  $("session-avatar").replaceChildren(avatar(j));
  const s = state.page;
  $("session-status").textContent =
    state.error ||
    (state.loading
      ? "正在读取会话…"
      : state.following
        ? `实时跟随 · ${time(s?.modified_at)} 更新`
        : "正在浏览历史记录");
  for (const button of $("session-history").querySelectorAll(
    ".history-button",
  ) as any)
    button.disabled = state.loading;
  const key = JSON.stringify([
    j.run_id,
    j.state,
    j.report,
    s,
    state.following,
    state.error,
  ]);
  if (key === state.key && !resetScroll) return;
  state.key = key;
  const root = $("session-history"),
    bottom = root.scrollHeight - root.clientHeight - root.scrollTop < 80,
    scroll = root.scrollTop;
  root.replaceChildren();
  const navigation = el("div", undefined, "history-navigation");
  if (s?.next_before != null) {
    const earlier = el("button", "↑ 更早的记录", "history-button");
    earlier.disabled = state.loading;
    earlier.addEventListener("click", () => fetchSession(s.next_before));
    navigation.append(earlier);
  } else if (s) navigation.append(el("span", "会话开始", "history-start"));
  if (state.older.length) {
    const newer = el("button", "较新的记录 ↓", "history-button");
    newer.disabled = state.loading;
    newer.addEventListener("click", () => {
      const before = state.older.at(-1);
      fetchSession(before, "newer");
    });
    navigation.append(newer);
  }
  if (state.error) {
    const retry = el("button", "重试", "history-button");
    retry.addEventListener("click", () =>
      fetchSession(
        state.following ? null : (state.page?.page_end ?? null),
        "retry",
      ),
    );
    navigation.append(retry);
  }
  root.append(navigation, processBlock(j, s));
  if (resetScroll) root.scrollTop = state.following ? root.scrollHeight : 0;
  else root.scrollTop = bottom && state.following ? root.scrollHeight : scroll;
}
