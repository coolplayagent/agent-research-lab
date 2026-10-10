"use strict";
const $ = (id) => document.getElementById(id),
  labels = {
    running: "运行中",
    succeeded: "已完成",
    completed: "已完成",
    active: "运行中",
    revoked: "已撤销",
    blocked: "阻塞",
    ready: "就绪",
    waiting_dependencies: "等待依赖",
    retry_wait: "等待重试",
    paused: "暂停",
    failed: "失败",
    unknown: "未知",
    needs_reconciliation: "待核对",
  };
const el = (tag, text, cls) => {
  const n = document.createElement(tag);
  if (text !== undefined) n.textContent = text;
  if (cls) n.className = cls;
  return n;
};
const badge = (state) =>
  el("span", labels[state] || state || "未知", `badge ${state || "unknown"}`);
const short = (s, n = 10) => (s || "").slice(0, n),
  time = (t) =>
    t ? new Date(t * 1000).toLocaleTimeString("zh-CN", { hour12: false }) : "—";
let data = null,
  room = "",
  selected = "",
  following = true,
  connected = false,
  stream,
  fetching = false,
  chatKey = "",
  chatView = "",
  chatRoute = "",
  detailKey = "",
  inspected = "",
  focusPanel = "",
  focusScroll = null,
  extraSession = null,
  sessionAgent = "",
  historyState = null,
  sessionReturnFocus = null;
const opened = new Set(),
  profiles = new Map(),
  pulses = new Map(),
  publishPulses = new Map(),
  lastBytes = new Map();
function role(j) {
  return j.id.startsWith("synthesis-") ? "synthesis" : j.role;
}
function name(j) {
  return j.profile?.display_name || j.id;
}
function roleLabel(j) {
  return (
    { research: "研究", review: "质疑", implement: "实现", synthesis: "综合" }[
      role(j)
    ] || j.role
  );
}
function avatar(j) {
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
function handle(j) {
  return j.profile?.handle || j.id;
}
function mention(j) {
  const button = el("button", `@${name(j)}`, "mention");
  button.title = `@${handle(j)} · ${j.id}`;
  button.addEventListener("click", () => openProfile(j.id));
  return button;
}
function openProfile(id) {
  if (!data.jobs.some((j) => j.id === id)) return;
  inspected = id;
  document.body.classList.add("profile-open");
  details(data.jobs.find((j) => j.id === id));
  $("close-profile").focus({ preventScroll: true });
}
function maximize(panel) {
  const next = focusPanel === panel ? "" : panel;
  if (next) document.body.classList.remove("profile-open");
  if (!focusPanel && next) focusScroll = $("timeline").scrollTop;
  focusPanel = next;
  document.body.classList.toggle("focus-graph", next === "graph");
  document.body.classList.toggle("focus-chat", next === "chat");
  for (const name of ["graph", "chat"]) {
    const button = $("maximize-" + name),
      active = next === name;
    button.textContent = active ? "↙ 还原" : "↗ 放大";
    button.setAttribute("aria-pressed", String(active));
    button.setAttribute(
      "aria-label",
      `${active ? "还原" : "放大"}${name === "graph" ? "协作图" : "对话窗口"}`,
    );
  }
  if (!next && focusScroll !== null) {
    const scroll = focusScroll;
    requestAnimationFrame(() => {
      $("timeline").scrollTop = scroll;
    });
    focusScroll = null;
  }
}
async function loadProfile(j) {
  if (profiles.has(j.run_id)) return;
  profiles.set(j.run_id, null);
  try {
    const r = await fetch(`/api/profile/${encodeURIComponent(j.run_id)}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!r.ok) throw new Error("profile");
    profiles.set(j.run_id, await r.json());
  } catch (_) {
    profiles.set(j.run_id, { error: "名片加载失败，可稍后重试。" });
  }
  if ((inspected || selected) === j.id) details(j);
}
function fold(key, title, cls) {
  const block = el("details", undefined, cls);
  block.dataset.fold = key;
  block.open = opened.has(key);
  block.addEventListener("toggle", () => {
    if (!block.isConnected) return;
    if (block.open) opened.add(key);
    else opened.delete(key);
  });
  block.append(el("summary", title));
  return block;
}
function readable(text, key, cls = "public-text") {
  const root = el("div", undefined, cls);
  if (text.length <= 900) root.append(el("div", text));
  else {
    root.append(el("div", text.slice(0, 600) + "…"));
    const more = fold(key, "展开全文", "message-more");
    more.append(el("div", text));
    root.append(more);
  }
  return root;
}
function cohortJobs() {
  return (data?.jobs || []).filter((j) => !room || j.cohort_id === room);
}
function jobs() {
  const q = $("search").value.toLowerCase().replace(/^@/, "");
  return cohortJobs().filter(
    (j) =>
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
function session(j) {
  return (
    (data.sessions || []).find((s) => s.run_id === j.run_id) ||
    (extraSession?.run_id === j.run_id ? extraSession : null)
  );
}
function choose(id, showSession = false) {
  selected = id;
  inspected = id;
  render();
  if (showSession) openSession(id);
}
function openSession(id) {
  const j = data?.jobs.find((job) => job.id === id);
  if (!j) return;
  sessionReturnFocus = document.activeElement;
  sessionAgent = id;
  historyState = {
    run: j.run_id,
    page: session(j),
    older: [],
    loading: false,
    following: true,
    error: "",
    key: "",
    request: 0,
  };
  document.body.classList.remove("profile-open");
  if (!$("session-window").open) $("session-window").showModal();
  renderSession(true);
  fetchSession();
}
function closeSession() {
  $("session-window").close();
  sessionAgent = "";
  historyState = null;
  if (sessionReturnFocus?.isConnected)
    sessionReturnFocus.focus({ preventScroll: true });
  else $("open-profile").focus({ preventScroll: true });
}
async function fetchSession(before = null, direction = "older") {
  const state = historyState;
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
    if (historyState !== state || request !== state.request) return;
    if (before !== null) {
      if (direction === "older") state.older.push(state.page?.page_end ?? null);
      if (direction === "newer") state.older.pop();
      state.following = direction === "newer" && !state.older.length;
    }
    if (before === null) state.following = true;
    state.page = page;
    extraSession = before === null ? page : extraSession;
    renderSession(before !== null);
  } catch (_) {
    if (historyState === state) state.error = "暂时无法读取会话，请稍后重试。";
  } finally {
    if (historyState === state) {
      state.loading = false;
      renderSession();
    }
  }
}
function renderSession(resetScroll = false) {
  const state = historyState,
    dialog = $("session-window");
  if (!state || !dialog.open) return;
  const j = data.jobs.find((job) => job.id === sessionAgent);
  if (!j || j.run_id !== state.run) {
    $("session-status").textContent =
      "这轮会话已结束或不在观察范围内，请重新选择数字人。";
    return;
  }
  $("session-title").textContent = name(j);
  $("session-subtitle").textContent =
    `${j.model} · ${labels[j.state] || j.state} · 第 ${j.attempt} 轮`;
  $("session-avatar").replaceChildren(avatar(j));
  const s = state.page;
  $("session-status").textContent =
    state.error ||
    (state.loading
      ? "正在读取会话…"
      : state.following
        ? `实时跟随 · ${time(s?.modified_at)} 更新`
        : "正在浏览历史记录");
  for (const button of $("session-history").querySelectorAll(".history-button"))
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
function svg(tag, attrs, text) {
  const n = document.createElementNS("http://www.w3.org/2000/svg", tag);
  Object.entries(attrs || {}).forEach(([k, v]) => n.setAttribute(k, v));
  if (text !== undefined) n.textContent = text;
  return n;
}
function graph(list) {
  const root = $("graph");
  root.replaceChildren();
  const ranked = [...list].sort(
    (a, b) => (a.state !== "running") - (b.state !== "running"),
  );
  const shown = ranked.slice(0, 8);
  $("graph-limit").textContent =
    list.length > 8
      ? `显示 ${shown.length}/${list.length} 个 agent；运行中的 session 优先`
      : "连线来自实际公共板记录；闪动表示新 session 活动";
  if (!shown.length) {
    root.append(el("div", "当前筛选下暂无 agent。", "empty"));
    return;
  }
  const canvas = svg("svg", {
      viewBox: "0 0 800 490",
      role: "group",
      "aria-label": "以水晶球公共板为中心的实时协作图",
    }),
    defs = svg("defs");
  const glow = svg("radialGradient", {
    id: "crystal-fill",
    cx: "35%",
    cy: "25%",
    r: "75%",
  });
  [
    ["0%", "#fff"],
    ["45%", "#dbf7f4"],
    ["85%", "#b5e4e1"],
    ["100%", "#d9eff0"],
  ].forEach(([offset, color]) =>
    glow.append(svg("stop", { offset, "stop-color": color })),
  );
  defs.append(glow);
  for (const [id, color] of [
    ["pub", "#3faaa0"],
    ["ctx", "#79a2c3"],
  ]) {
    const m = svg("marker", {
      id,
      viewBox: "0 0 10 10",
      refX: 9,
      refY: 5,
      markerWidth: 5,
      markerHeight: 5,
      orient: "auto",
    });
    m.append(svg("path", { d: "M0 0 L10 5 L0 10 z", fill: color }));
    defs.append(m);
  }
  canvas.append(defs);
  canvas.append(
    svg("ellipse", {
      cx: 400,
      cy: 240,
      rx: 288,
      ry: 173,
      fill: "none",
      stroke: "#e2eeee",
      "stroke-dasharray": "3 7",
    }),
    svg("circle", {
      cx: 400,
      cy: 240,
      r: 104,
      fill: "none",
      stroke: "#deeeec",
    }),
  );
  const board = data.boards.find((b) => b.cohort_id === room),
    allMessages = board?.retained_messages || [];
  const positions = [];
  shown.forEach((j, i) => {
    const angle = -Math.PI / 2 + (i * Math.PI * 2) / shown.length;
    const x = 400 + 286 * Math.cos(angle),
      y = 240 + 181 * Math.sin(angle);
    positions.push({ j, x, y, angle });
    const x1 = x - 62 * Math.cos(angle),
      y1 = y - 32 * Math.sin(angle),
      x2 = 400 + 88 * Math.cos(angle),
      y2 = 240 + 88 * Math.sin(angle);
    const path = `M${x1},${y1} L${x2},${y2}`;
    canvas.append(svg("path", { d: path, class: "membership" }));
    const count = allMessages.filter((m) => m.message.task_id === j.id).length;
    if (count)
      canvas.append(
        svg("path", { d: path, class: "publish", "marker-end": "url(#pub)" }),
      );
    if (
      j.context?.message_ids?.length ||
      allMessages.some((m) =>
        (m.message.proposal.recipients || []).includes(j.id),
      )
    )
      canvas.append(
        svg("path", {
          d: `M${x2 + 4},${y2 + 4} L${x1 + 4},${y1 + 4}`,
          class: "context",
          "marker-end": "url(#ctx)",
        }),
      );
    if ((publishPulses.get(j.id) || 0) > Date.now())
      canvas.append(svg("path", { d: path, class: "traffic" }));
  });
  const orb = svg("g", {
    class: "board",
    tabindex: 0,
    role: "button",
    "aria-label": "打开水晶球公共板",
  });
  orb.append(
    svg("circle", {
      cx: 400,
      cy: 240,
      r: 86,
      fill: "url(#crystal-fill)",
      stroke: "#97cfca",
      "stroke-width": 1,
    }),
    svg("ellipse", {
      cx: 382,
      cy: 205,
      rx: 37,
      ry: 20,
      fill: "#fff",
      opacity: 0.45,
      transform: "rotate(-25 382 205)",
    }),
    svg("text", { x: 400, y: 225 }, "水晶球"),
    svg("text", { x: 400, y: 247 }, "消息中心"),
    svg(
      "text",
      { x: 400, y: 269, class: "board-sub" },
      `${allMessages.filter((m) => !(m.message.proposal.recipients || []).length).length} 条公共 / ${allMessages.filter((m) => (m.message.proposal.recipients || []).length).length} 条定向`,
    ),
  );
  const open = () => {
    if (focusPanel === "graph") maximize("chat");
    chatRoute = "";
    chatKey = "";
    render();
    $("timeline").scrollTop = $("timeline").scrollHeight;
  };
  orb.addEventListener("click", open);
  orb.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      open();
    }
  });
  canvas.append(orb);
  positions.forEach(({ j, x, y }) => {
    const s = session(j),
      event = s?.events?.at(-1);
    const busy = (pulses.get(j.run_id) || 0) > Date.now();
    const group = svg("g", {
      class: `node ${j.id === selected ? "selected" : ""} ${busy ? "busy" : ""}`,
      transform: `translate(${x - 78},${y - 32})`,
      tabindex: 0,
      role: "button",
      "aria-label": `${j.id} ${labels[j.state]}`,
    });
    group.append(
      svg("title", {}, j.id),
      svg("rect", { width: 156, height: 64, rx: 8 }),
      svg("circle", {
        cx: 13,
        cy: 16,
        r: 3,
        fill:
          j.state === "running"
            ? "#199377"
            : j.state === "blocked"
              ? "#c79b56"
              : "#b4c6c8",
      }),
      svg("text", { x: 23, y: 20 }, name(j)),
      svg(
        "text",
        { x: 12, y: 37, class: "node-sub" },
        `${roleLabel(j)} · ${j.model} · ${labels[j.state] || j.state}`,
      ),
      svg(
        "text",
        { x: 12, y: 52, class: "node-activity" },
        event?.title ||
          (j.state === "ready" ? "等待建立 session" : "暂无 session 活动"),
      ),
    );
    const inspect = () => {
      choose(j.id, true);
    };
    group.addEventListener("click", inspect);
    group.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        inspect();
      }
    });
    canvas.append(group);
  });
  root.append(canvas);
}
function details(j) {
  const root = $("detail"),
    s = j && session(j),
    profile = j && profiles.get(j.run_id);
  const key = JSON.stringify([j, s?.bytes, profile]);
  if (key === detailKey) return;
  detailKey = key;
  const scroll = root.parentElement.scrollTop;
  root.replaceChildren();
  if (!j) {
    root.append(
      el("div", "选择数字人，查看身份、角色准则和协作记录。", "empty"),
    );
    return;
  }
  const hero = el("div", undefined, "persona-hero"),
    title = el("div");
  title.append(el("h2", name(j)), el("code", "@" + handle(j)), badge(j.state));
  hero.append(avatar(j), title);
  root.append(hero, el("p", `${j.model} · ${j.backend}`, "persona-model"));
  const inspect = el("button", "打开会话历史", "inspect-process");
  inspect.addEventListener("click", () => {
    document.body.classList.remove("profile-open");
    if (j.cohort_id !== room) {
      room = j.cohort_id || "";
      following = false;
    }
    $("search").value = "";
    $("state").value = "";
    choose(j.id, true);
  });
  root.append(inspect, el("h3", "Soul · 角色准则"));
  if (!profiles.has(j.run_id)) loadProfile(j);
  if (profile?.soul?.available) {
    root.append(
      readable(profile.soul.content, `soul:${j.run_id}`, "soul-text"),
    );
    const source = profile.soul.source;
    root.append(
      el(
        "p",
        source.kind === "frozen_source"
          ? `${source.path} @ ${short(source.commit, 12)}`
          : `已注册策略 ${source.version}`,
        "process-meta",
      ),
    );
    root.append(el("code", `角色准则 SHA-256 ${profile.soul.sha256}`));
  } else
    root.append(
      el(
        "p",
        profile?.error || profile?.soul?.reason || "正在读取绑定的角色准则…",
      ),
    );
  if (profile?.error || profile?.soul?.available === false) {
    const retry = el("button", "重新读取名片", "mention");
    retry.addEventListener("click", () => {
      profiles.delete(j.run_id);
      detailKey = "";
      details(j);
    });
    root.append(retry);
  }
  root.append(
    el(
      "p",
      "Soul 来自任务的角色准则；尚无独立人格配置。身份按任务区分，同一任务重试沿用名片。",
      "process-meta",
    ),
  );
  root.append(el("h3", "职责与当前任务"));
  const dl = el("dl");
  for (const [k, v] of [
    ["花名", name(j)],
    ["任务", j.id],
    ["角色", role(j)],
    ["模型", j.model],
    ["后端", j.backend],
    ["工作范围", `${j.repository} · ${j.write ? "候选写入" : "只读研究"}`],
    ["记忆", j.profile?.use_memory ? "本任务隔离记忆" : "本任务未启用"],
    ["必需工具", j.profile?.required_tools?.join("、") || "未指定"],
    ["当前活动", s?.events?.at(-1)?.title || "暂无公开活动"],
    ["日志更新", time(s?.modified_at)],
    ["执行轮次", `${j.attempt} / ${j.profile?.max_attempts || 3}`],
    ["Session", s?.session_id || "尚无会话 ID"],
    ["前置任务", j.dependencies.join("、") || "无"],
  ])
    dl.append(el("dt", k), el("dd", v));
  root.append(dl);
  if (j.reason)
    root.append(el("h3", "主机阻塞记录"), el("div", j.reason, "reason"));
  if (j.observation_error) root.append(el("p", "workflow 状态暂时不可读。"));
  const records = data.boards.flatMap((b) => b.retained_messages || []),
    sent = records.filter((m) => m.message.task_id === j.id),
    ids = j.context?.message_ids || [];
  root.append(
    el("h3", "协作看板"),
    el(
      "p",
      `已发布 ${sent.length} 条留存消息 · 已注入 ${ids.length} 条公共板提议`,
    ),
  );
  if (j.report?.summary) {
    root.append(
      el("h3", "已提交的研究结果"),
      readable(j.report.summary, `report-summary:${j.run_id}`),
    );
    const report = fold(
      `report:${j.run_id}`,
      "研究发现与局限",
      "profile-section",
    );
    for (const text of j.report.findings || [])
      if (text) report.append(el("p", "发现：" + text));
    for (const text of j.report.limitations || [])
      if (text) report.append(el("p", "局限：" + text));
    root.append(report);
  }
  const evidence = fold(
    `evidence:${j.run_id}`,
    "版本与证据绑定",
    "profile-section",
  );
  for (const [k, v] of [
    ["运行 ID", j.run_id],
    ["任务源码", j.source_commit],
    ["SuperPOD", j.superpod_commit],
    ["Prompt SHA-256", j.prompt_digest],
    ["配置 / 策略 SHA-256", j.config_digest],
    ["候选提交", j.candidate_commit],
  ]) {
    if (!v) continue;
    const b = el("div", undefined, "binding");
    b.append(el("span", k), el("code", v));
    evidence.append(b);
  }
  ids.forEach((id) => evidence.append(el("p", id)));
  if (j.context?.digest) evidence.append(el("code", j.context.digest));
  root.append(
    evidence,
    el(
      "p",
      "上下文注入表示传递；是否采纳仍需核对。留存消息数受当前观察窗口限制。",
      "process-meta",
    ),
  );
  root.parentElement.scrollTop = scroll;
}
function isError(e) {
  return (
    ["turn.failed", "error"].includes(e.type) ||
    ["failed", "error"].includes(e.status) ||
    (e.exit_code != null && e.exit_code !== 0)
  );
}
function eventDetails(j, e) {
  const id = `${j.run_id}:${e.id}`,
    row = fold(
      id,
      `${isError(e) ? "! " : ""}${e.title}${e.exit_code != null ? " · exit " + e.exit_code : ""}`,
      "process-event",
    );
  if (e.text) row.append(readable(e.text, `${id}:text`));
  if (e.command) row.append(el("pre", e.command));
  if (e.output) row.append(el("pre", e.output));
  const raw = fold(`${id}:json`, "查看归一化 JSON", "protocol-detail");
  raw.append(el("pre", JSON.stringify(e, null, 2)));
  row.append(raw);
  return row;
}
function processBlock(j, s) {
  const block = el("div", undefined, "session-transcript"),
    body = el("div", undefined, "process-body"),
    events = s?.events || [];
  if (!s) body.append(el("p", "这位数字人还没有可读取的会话记录。", "empty"));
  else if (!events.length)
    body.append(el("p", "这一段没有公开对话，可继续查看其他记录。", "empty"));
  let tools = [];
  const flush = () => {
    if (!tools.length) return;
    const failed = tools.filter(isError).length,
      group = fold(
        `tools:${j.run_id}:${tools[0].id}`,
        `执行过程 · ${tools.length} 条活动${failed ? ` · ${failed} 条异常` : ""}`,
        "tool-group",
      );
    group.firstChild.append(el("small", tools.at(-1).title));
    if (failed) {
      group.classList.add("has-errors");
      body.append(
        el("p", "部分操作报告异常，可展开执行过程查看原因。", "activity-error"),
      );
    }
    tools.forEach((e) => group.append(eventDetails(j, e)));
    body.append(group);
    tools = [];
  };
  for (const e of events) {
    const speaking =
      e.item_type === "agent_message" ||
      (e.type === "session.activity" && e.text);
    if (speaking) {
      flush();
      const message = el("article", undefined, "bot-message");
      message.append(el("div", name(j), "bot-label"));
      let structured = null;
      try {
        structured = JSON.parse(e.text);
      } catch (_) {}
      const text =
        typeof structured?.summary === "string"
          ? structured.summary
          : e.text || "未提供正文";
      message.append(readable(text, `${j.run_id}:${e.id}:message`));
      const raw = fold(
        `${j.run_id}:${e.id}:json`,
        "消息详情 · stdio-json",
        "protocol-detail",
      );
      raw.append(el("pre", JSON.stringify(e, null, 2)));
      message.append(raw);
      body.append(message);
    } else if (
      ["thread.started", "turn.started", "turn.completed"].includes(e.type)
    ) {
      flush();
      body.append(el("div", e.title, "lifecycle"));
    } else if (["turn.failed", "error"].includes(e.type)) {
      flush();
      body.append(
        el("div", e.text || e.title, "activity-error"),
        eventDetails(j, e),
      );
    } else tools.push(e);
  }
  flush();
  if (historyState?.following && j.report?.summary) {
    const result = el("article", undefined, "bot-message result-message");
    result.append(
      el("div", "已提交的研究结果", "bot-label"),
      readable(j.report.summary, `result:${j.run_id}`),
    );
    const more = fold(
      `result-details:${j.run_id}`,
      "查看研究发现与局限",
      "message-more",
    );
    for (const text of j.report.findings || [])
      if (text) more.append(el("p", text));
    for (const text of j.report.limitations || [])
      if (text) more.append(el("p", "局限：" + text));
    result.append(more);
    body.append(result);
  }
  body.append(
    el("p", "仅显示公开会话；工具调用与 JSON 可按需展开。", "process-meta"),
  );
  if (s?.invalid_lines)
    body.append(
      el("p", `${s.invalid_lines} 行日志无法解析，已跳过。`, "process-meta"),
    );
  block.append(body);
  return block;
}
function channelKey(record) {
  const recipients = record.message.proposal.recipients || [];
  if (!recipients.length) return "";
  return JSON.stringify([
    record.cohort_id,
    [...new Set([record.message.task_id, ...recipients])].sort(),
  ]);
}
function conversationRecords() {
  return data.boards
    .filter((b) => !room || b.cohort_id === room)
    .flatMap((b) =>
      (b.retained_messages || []).map((r) => ({
        ...r,
        cohort_id: b.cohort_id,
      })),
    );
}
function privateChats() {
  const byId = new Map(data.jobs.map((j) => [j.id, j])),
    channels = new Map();
  for (const record of conversationRecords()) {
    const key = channelKey(record);
    if (!key) continue;
    if (!channels.has(key)) {
      const ids = [
        ...new Set([
          record.message.task_id,
          ...record.message.proposal.recipients,
        ]),
      ].sort();
      channels.set(key, {
        label: ids
          .map((id) => (byId.has(id) ? name(byId.get(id)) : id))
          .join("、"),
        count: 0,
      });
    }
    channels.get(key).count++;
  }
  if (chatRoute && !channels.has(chatRoute)) chatRoute = "";
  const root = $("private-chats"),
    scroll = root.scrollTop;
  root.replaceChildren();
  $("private-count").textContent = channels.size || "";
  for (const [key, channel] of channels) {
    const button = el(
      "button",
      undefined,
      `private-chat ${chatRoute === key ? "selected" : ""}`,
    );
    button.append(
      el("span", "◈", "private-icon"),
      el("span", channel.label, "private-name"),
      el("small", channel.count),
    );
    button.title = channel.label;
    button.addEventListener("click", () => {
      chatRoute = key;
      following = false;
      render();
    });
    root.append(button);
  }
  if (!channels.size)
    root.append(el("p", "私聊与多人会话会显示在这里", "private-empty"));
  root.scrollTop = scroll;
  $("chat-title").textContent = chatRoute
    ? `◈ ${channels.get(chatRoute).label}`
    : "# 协作群聊";
  $("chat-title").title = chatRoute
    ? "水晶球定向分发 · 点击返回公共群聊"
    : "水晶球公共群聊";
}
function timeline(list) {
  const root = $("timeline"),
    ids = new Set(list.map((j) => j.id)),
    byId = new Map(data.jobs.map((j) => [j.id, j]));
  const allRecords = conversationRecords();
  const records = allRecords
    .filter((m) => channelKey(m) === chatRoute && ids.has(m.message.task_id))
    .sort((a, b) => a.message.created_at - b.message.created_at);
  const sources = new Map(allRecords.map((m) => [m.message.id, m]));
  const key = JSON.stringify([
    room,
    chatRoute,
    records,
    list.map((j) => [j.id, name(j)]),
  ]);
  if (key === chatKey) return;
  chatKey = key;
  const view = `${room}:${chatRoute}`;
  const changedView = view !== chatView;
  chatView = view;
  const bottom = root.scrollHeight - root.clientHeight - root.scrollTop < 80,
    scroll = root.scrollTop;
  root.replaceChildren();
  $("message-count").textContent = `${records.length} 条消息`;
  if (!records.length)
    root.append(
      el("div", "还没有群聊消息。数字人的会话可从左侧头像打开。", "empty"),
    );
  for (const record of records.slice(-256)) {
    const m = record.message,
      p = m.proposal,
      j = byId.get(m.task_id);
    const row = el("article", undefined, "chat-message");
    row.id = "msg-" + m.id;
    const face = el("button", undefined, "avatar-button"),
      body = el("div", undefined, "chat-message-body"),
      sender = el("button", name(j), "sender");
    face.setAttribute("aria-label", `打开 ${name(j)} 的会话`);
    face.append(avatar(j));
    face.addEventListener("click", () => choose(j.id, true));
    sender.addEventListener("click", () => choose(j.id, true));
    const bubble = el("div", undefined, "chat-bubble");
    if (record.origin.state === "revoked") {
      bubble.append(el("p", "这条消息已撤销。", "withdrawn-message"));
      bubble.title = record.origin.reason || "";
    } else {
      if (p.recipients?.length) {
        for (const id of p.recipients) {
          const recipient = byId.get(id);
          bubble.append(
            recipient
              ? mention(recipient)
              : el("span", `@${id}`, "mention unavailable"),
          );
        }
      } else if (p.reply_to) {
        const parent = sources.get(p.reply_to),
          recipient = byId.get(parent?.message.task_id);
        if (recipient) bubble.append(mention(recipient));
        else
          bubble.append(
            el(
              "span",
              parent ? `@${parent.message.task_id}` : "@原消息作者",
              "mention unavailable",
            ),
          );
      }
      bubble.append(readable(p.text, `board-message:${m.id}`));
      if (record.expired) bubble.title = "历史消息，已过期";
    }
    body.append(sender, bubble);
    row.append(face, body);
    root.append(row);
  }
  root.scrollTop = changedView || bottom ? root.scrollHeight : scroll;
}
function rooms() {
  const container = $("rooms");
  container.replaceChildren();
  const groups = new Map();
  data.jobs.forEach((j) => {
    if (j.cohort_id && !groups.has(j.cohort_id))
      groups.set(j.cohort_id, j.team || short(j.cohort_id));
  });
  $("room-count").textContent = groups.size;
  const add = (id, label) => {
    const list = data.jobs.filter((j) => !id || j.cohort_id === id),
      button = el(
        "button",
        undefined,
        `room-button ${id === room ? "selected" : ""}`,
      ),
      text = el("span", label, "room-name");
    text.append(
      el(
        "small",
        `${list.filter((j) => j.state === "running").length} 个活跃 · ${short(id) || "包括历史记录"}`,
      ),
    );
    button.append(
      el("span", "#", "room-icon"),
      text,
      el("span", list.length, "room-count"),
    );
    button.title = label;
    button.addEventListener("click", () => {
      room = id;
      inspected = "";
      following = false;
      chatRoute = "";
      chatKey = "";
      render();
    });
    container.append(button);
  };
  groups.forEach((label, id) => add(id, label));
  add("", "全部历史任务");
  $("latest-room").classList.toggle("following", following);
}
function render() {
  if (!data) return;
  const list = jobs(),
    all = cohortJobs();
  if (!list.some((j) => j.id === selected)) selected = list[0]?.id || "";
  rooms();
  privateChats();
  const group = data.jobs.find((j) => j.cohort_id === room);
  $("room-title").textContent = room
    ? "# " + (group?.team || short(room))
    : "# 全部研究记录";
  $("room-subtitle").textContent =
    `${all.length} 个任务 · ${data.max_agents || 8} 个并发上限 · SuperPOD 公共知识库`;
  const count = (states) => all.filter((j) => states.includes(j.state)).length;
  $("running").textContent = count(["running"]);
  $("pending").textContent = count([
    "ready",
    "waiting_dependencies",
    "retry_wait",
  ]);
  $("succeeded").textContent = count(["succeeded"]);
  $("blocked").textContent = count([
    "blocked",
    "failed",
    "needs_reconciliation",
    "unknown",
  ]);
  $("history-count").textContent =
    `全库 ${data.jobs.length} 个任务 · 历史阻塞 ${data.summary?.blocked || 0} 个`;
  const control = data.controller || {},
    phases = {
      starting: "启动中",
      recovering: "恢复历史任务",
      refreshing: "核对最新版本",
      idle: "待命",
      running: "研究运行中",
      stopped: "已停止",
      unknown: "状态未上报",
    };
  const failed = !!control.seed?.error;
  const problems = [
    data.error,
    control.stale
      ? `控制器心跳已延迟 ${control.heartbeat_age_seconds} 秒，正在持续观察。`
      : null,
    ...new Set(
      (data.warnings || []).filter(
        (w) => !room || !w.startsWith("协作组 ") || w.includes(short(room, 12)),
      ),
    ),
    failed
      ? `新研究准入受阻：${control.seed.error}${control.seed.next_attempt_at ? " · 下次检查 " + time(control.seed.next_attempt_at) : ""}`
      : null,
  ].filter(Boolean);
  $("notice").hidden = !problems.length;
  $("notice").textContent = problems.join("\n");
  $("controller-status").textContent = control.live
    ? failed
      ? "版本检查待恢复"
      : phases[control.phase] || control.phase
    : "控制器离线 / 未上报";
  $("controller-status").classList.toggle(
    "problem",
    failed || !control.live || control.stale,
  );
  const rows = $("tasks");
  const scroll = rows.scrollTop;
  rows.replaceChildren();
  list.forEach((j) => {
    const b = el(
        "button",
        undefined,
        `task-row ${j.id === selected ? "selected" : ""}`,
      ),
      text = el("span", name(j), "task-name");
    text.append(
      el(
        "small",
        `${roleLabel(j)} · ${j.model} · ${labels[j.state] || j.state}`,
      ),
    );
    b.append(avatar(j), text, el("span", undefined, `presence ${j.state}`));
    b.title = j.id;
    b.addEventListener("click", () => choose(j.id, true));
    rows.append(b);
  });
  rows.scrollTop = scroll;
  $("task-count").textContent = list.length;
  graph(list);
  details(data.jobs.find((j) => j.id === (inspected || selected)));
  timeline(list);
  renderSession();
  connection();
}
function accept(value) {
  value.jobs = value.jobs || [];
  value.boards = value.boards || [];
  value.sessions = value.sessions || [];
  const knownRuns = new Set(value.jobs.map((j) => j.run_id));
  for (const id of profiles.keys()) if (!knownRuns.has(id)) profiles.delete(id);
  for (const s of value.sessions) {
    const old = lastBytes.get(s.run_id);
    if (old !== undefined && old !== s.bytes)
      pulses.set(s.run_id, Date.now() + 1600);
    lastBytes.set(s.run_id, s.bytes);
  }
  if (data) {
    const known = new Set(
      data.boards.flatMap((b) =>
        (b.retained_messages || []).map((m) => m.message.id),
      ),
    );
    for (const b of value.boards)
      for (const m of b.retained_messages || [])
        if (!known.has(m.message.id))
          publishPulses.set(m.message.task_id, Date.now() + 2000);
  }
  data = value;
  if (!data.jobs.some((j) => j.id === inspected)) inspected = "";
  if (following) {
    const active = data.jobs.find((j) => j.state === "running" && j.cohort_id),
      seed = data.jobs.find(
        (j) =>
          (data.controller?.seed?.queued || []).includes(j.id) && j.cohort_id,
      );
    room =
      active?.cohort_id ||
      seed?.cohort_id ||
      data.jobs.find((j) => j.cohort_id)?.cohort_id ||
      "";
  }
  render();
}
function connection() {
  const age = data?.sampled_at
    ? Math.max(0, Math.floor(Date.now() / 1000 - data.sampled_at))
    : null;
  const healthy = connected && age !== null && age < 40 && !data.error;
  $("connection-dot").classList.toggle("live", healthy);
  $("connection").textContent = healthy
    ? "实时连接"
    : connected
      ? "正在采样"
      : "正在重连";
  $("sample-time").textContent =
    age === null ? "等待快照" : `任务快照 ${age} 秒前 · session 每秒观察`;
}
async function refresh() {
  if (fetching) return;
  fetching = true;
  $("refresh").disabled = true;
  try {
    const r = await fetch("/api/snapshot", {
      cache: "no-store",
      signal: AbortSignal.timeout(8000),
    });
    if (!r.ok) throw new Error("snapshot");
    accept(await r.json());
  } catch (_) {
    connected = false;
    connection();
  } finally {
    fetching = false;
    $("refresh").disabled = false;
  }
}
function connect() {
  stream = new EventSource("/api/events");
  stream.addEventListener("snapshot", (e) => {
    try {
      connected = true;
      accept(JSON.parse(e.data));
    } catch (_) {
      connected = false;
      connection();
    }
  });
  stream.onopen = () => {
    connected = true;
    connection();
  };
  stream.onerror = () => {
    connected = false;
    connection();
  };
}
$("open-profile").addEventListener("click", () => openProfile(selected));
$("close-profile").addEventListener("click", () => {
  document.body.classList.remove("profile-open");
  $("open-profile").focus();
});
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || $("session-window").open) return;
  if (document.body.classList.contains("profile-open"))
    document.body.classList.remove("profile-open");
  else if (focusPanel) maximize(focusPanel);
});
$("maximize-graph").addEventListener("click", () => maximize("graph"));
$("maximize-chat").addEventListener("click", () => maximize("chat"));
$("search").addEventListener("input", render);
$("state").addEventListener("change", render);
$("refresh").addEventListener("click", refresh);
$("latest-room").addEventListener("click", () => {
  chatRoute = "";
  following = true;
  if (data) accept(data);
});
$("view-map").addEventListener("click", () => {
  document.body.classList.remove("chat-only");
  $("view-map").classList.add("active");
  $("view-chat").classList.remove("active");
});
$("view-chat").addEventListener("click", () => {
  document.body.classList.add("chat-only");
  $("view-chat").classList.add("active");
  $("view-map").classList.remove("active");
});
$("chat-title").addEventListener("click", () => {
  chatRoute = "";
  render();
});
$("close-session").addEventListener("click", closeSession);
$("session-window").addEventListener("cancel", (e) => {
  e.preventDefault();
  closeSession();
});
$("maximize-session").addEventListener("click", () => {
  const expanded = $("session-window").classList.toggle("expanded");
  $("maximize-session").textContent = expanded ? "↙ 还原" : "↗ 放大";
  $("maximize-session").setAttribute("aria-pressed", String(expanded));
  $("maximize-session").setAttribute(
    "aria-label",
    `${expanded ? "还原" : "放大"}会话窗口`,
  );
});
$("session-profile").addEventListener("click", () => {
  const id = sessionAgent;
  closeSession();
  openProfile(id);
});
$("session-latest").addEventListener("click", () => {
  if (!historyState || historyState.loading) return;
  historyState.older = [];
  historyState.following = true;
  historyState.key = "";
  fetchSession();
  $("session-history").scrollTop = $("session-history").scrollHeight;
});
setInterval(() => {
  if (historyState?.following && $("session-window").open) fetchSession();
}, 3000);
setInterval(connection, 1000);
setInterval(() => {
  if (!connected) refresh();
}, 10000);
refresh();
connect();
