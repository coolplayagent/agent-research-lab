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
  chatMode = "public",
  connected = false,
  stream,
  fetching = false,
  chatKey = "",
  chatView = "",
  detailKey = "",
  inspected = "",
  focusPanel = "",
  focusScroll = null,
  extraSession = null;
const opened = new Set(),
  profiles = new Map(),
  pulses = new Map(),
  publishPulses = new Map(),
  lastBytes = new Map();
function role(j) {
  return j.id.startsWith("synthesis-") ? "synthesis" : j.role;
}
function name(j) {
  const phase =
    { research: "研究", review: "质疑", implement: "实现", synthesis: "综合" }[
      role(j)
    ] || j.role;
  return `${phase} · ${(j.topics || [])[0] || j.repository}`;
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
  const button = el(
    "button",
    `@${name(j)} · ${handle(j).slice(-6)}`,
    "mention",
  );
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
async function choose(id, process = false) {
  selected = id;
  inspected = id;
  if (process) {
    if (focusPanel === "graph") maximize("chat");
    chatMode = "process";
    const j = data.jobs.find((j) => j.id === id);
    if (j) opened.add(`process:${j.run_id}`);
  }
  render();
  const j = data.jobs.find((j) => j.id === id);
  if (j && !session(j)) {
    try {
      const r = await fetch(`/api/session/${encodeURIComponent(j.run_id)}`, {
        signal: AbortSignal.timeout(5000),
      });
      if (r.ok) {
        extraSession = await r.json();
        if (selected === id) render();
      }
    } catch (_) {}
  }
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
    if (j.context?.message_ids?.length)
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
    svg("text", { x: 400, y: 247 }, "公共板"),
    svg(
      "text",
      { x: 400, y: 269, class: "board-sub" },
      `${board?.active_messages || 0} 条可用提议 / ${board?.accepted_total || 0} 条留存`,
    ),
  );
  const open = () => {
    if (focusPanel === "graph") maximize("chat");
    chatMode = "public";
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
        `${j.model} · ${labels[j.state] || j.state}`,
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
      openProfile(j.id);
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
  const inspect = el("button", "查看 TA 的对话与过程", "inspect-process");
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
  const key = `process:${j.run_id}`,
    block = fold(key, `${name(j)} · 对话与执行过程`, "process-block");
  const events = s?.events || [],
    errors = events.filter(isError);
  block.firstChild.append(
    el(
      "small",
      `${events.filter((e) => e.item_type === "agent_message").length} 条发言 · ${events.length} 条活动${errors.length ? ` · ${errors.length} 条异常` : ""}`,
    ),
  );
  if (errors.length) block.classList.add("has-errors");
  const body = el("div", undefined, "process-body"),
    identity = el("div", undefined, "process-identity");
  identity.append(avatar(j), mention(j), badge(j.state));
  body.append(identity);
  if (!s)
    body.append(el("p", "该轮尚未生成可读取的活动记录。", "process-meta"));
  else
    body.append(
      el(
        "p",
        `Session ${s.session_id || "未提供"} · 日志更新 ${time(s.modified_at)}${s.truncated ? " · 当前仅为最近活动片段" : ""}`,
        "process-meta",
      ),
    );
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
      message.append(el("div", "Agent 发言", "bot-label"));
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
  if (j.report?.summary) {
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
    el(
      "p",
      "发言与进度来自 worker 公开事件，未经独立验证。命令、输出与 JSON 默认折叠；原始事件未提供时间戳。",
      "process-meta",
    ),
  );
  if (s?.invalid_lines)
    body.append(
      el("p", `${s.invalid_lines} 行日志无法解析，已跳过。`, "process-meta"),
    );
  block.append(body);
  return block;
}
function timeline(list) {
  const root = $("timeline"),
    ids = new Set(list.map((j) => j.id)),
    byId = new Map(data.jobs.map((j) => [j.id, j]));
  const boards = data.boards.filter((b) => !room || b.cohort_id === room);
  const records = boards
    .flatMap((b) => b.retained_messages || [])
    .filter((m) => ids.has(m.message.task_id));
  const sources = new Map(
    boards
      .flatMap((b) => b.retained_messages || [])
      .map((m) => [m.message.id, m]),
  );
  const key = JSON.stringify([
    room,
    chatMode,
    selected,
    $("search").value,
    $("state").value,
    records.map((m) => [m.message.id, m.origin.state, m.expired]),
    list.map((j) => [j.id, j.state, session(j)?.bytes, j.context?.digest]),
  ]);
  if (key === chatKey) return;
  chatKey = key;
  const view = `${room}:${chatMode}:${chatMode === "process" ? selected : ""}`,
    changedView = view !== chatView;
  chatView = view;
  const bottom = root.scrollHeight - root.clientHeight - root.scrollTop < 80,
    scroll = root.scrollTop;
  root.replaceChildren();
  $("public-chat").classList.toggle("active", chatMode === "public");
  $("session-chat").classList.toggle("active", chatMode === "process");
  if (chatMode === "process") {
    const j = data.jobs.find((j) => j.id === selected) || list[0];
    if (j) root.append(processBlock(j, session(j)));
    else root.append(el("div", "选择一个 agent session。", "empty"));
    $("message-count").textContent = "可折叠的执行记录";
  } else {
    const items = records.map((m) => ({
      at: m.message.created_at,
      kind: "message",
      record: m,
    }));
    list.forEach((j) => {
      if (j.context?.message_ids?.length)
        items.push({ at: j.context.as_of, kind: "context", job: j });
    });
    items.sort((a, b) => a.at - b.at);
    $("message-count").textContent = `${records.length} 条公共板消息`;
    if (!items.length)
      root.append(
        el(
          "div",
          "公共板等待新的协作提议。选择左侧数字人，查看它的对话与执行过程。",
          "empty",
        ),
      );
    for (const item of items.slice(-256)) {
      const j = item.job ||
        byId.get(item.record.message.task_id) || {
          id: item.record.message.task_id,
          role: "research",
          repository: "",
          topics: [],
        };
      const row = el(
        "article",
        undefined,
        `event ${role(j)} ${item.record?.expired ? "expired" : ""}`,
      );
      if (item.record) row.id = "msg-" + item.record.message.id;
      const face = el("button", undefined, "avatar-button");
      face.setAttribute(
        "aria-label",
        item.kind === "context" ? "水晶球公共板" : `查看 ${name(j)} 的名片`,
      );
      if (item.kind === "context")
        face.append(el("span", "◈", "avatar board-avatar"));
      else {
        face.append(avatar(j));
        face.addEventListener("click", () => openProfile(j.id));
      }
      row.append(face);
      const body = el("div", undefined, "event-body"),
        head = el("div", undefined, "event-head"),
        sender = el(
          "button",
          item.kind === "context" ? "水晶球" : name(j),
          "sender",
        );
      sender.title = j.id;
      sender.addEventListener("click", () =>
        item.kind === "context" ? $("public-chat").click() : openProfile(j.id),
      );
      head.append(sender, el("span", "→", "direction"));
      if (item.kind === "context") head.append(mention(j));
      else {
        const parentId = item.record.message.proposal.reply_to,
          parent = sources.get(parentId),
          recipient = byId.get(parent?.message.task_id);
        if (recipient) head.append(mention(recipient));
        else if (parentId)
          head.append(
            el(
              "span",
              parent ? `@${parent.message.task_id}` : "回复对象未保留",
              "mention unavailable",
            ),
          );
        else {
          const board = el("button", "@公共板", "mention");
          board.addEventListener("click", () => $("public-chat").click());
          head.append(board);
        }
      }
      head.append(el("time", time(item.at)));
      body.append(head);
      if (item.kind === "context") {
        head.append(el("span", "上下文投递 · 非发言", "chip"));
        body.append(
          el(
            "div",
            `启动时收到 ${j.context.message_ids.length} 条公共板提议。`,
            "context-event",
          ),
        );
        j.context.message_ids.forEach((id) => {
          const b = el(
            "button",
            `↳ ${sources.get(id)?.message.proposal.text.slice(0, 75) || short(id, 24)}`,
            "reply",
          );
          b.addEventListener("click", () =>
            document
              .getElementById("msg-" + id)
              ?.scrollIntoView({ block: "center" }),
          );
          body.append(b);
        });
      } else {
        const m = item.record.message,
          p = m.proposal;
        head.append(
          el("span", p.reply_to ? "群内回复" : "群内广播", "audience"),
        );
        head.append(
          el(
            "span",
            {
              finding: "发现",
              question: "问题",
              counterexample: "反例",
              reference: "参考",
            }[p.kind] || p.kind,
            "chip",
          ),
          badge(item.record.origin.state),
        );
        if (item.record.expired) head.append(el("span", "已过期", "badge"));
        const bubble = el("div", undefined, "bubble");
        if (p.reply_to) {
          const parent = sources.get(p.reply_to),
            reply = el(
              "button",
              `回复 ${parent?.message.task_id || short(p.reply_to)}\n${parent?.message.proposal.text.slice(0, 160) || "原消息不在当前保留窗口"}`,
              "reply",
            );
          reply.addEventListener("click", () =>
            document
              .getElementById("msg-" + p.reply_to)
              ?.scrollIntoView({ block: "center" }),
          );
          bubble.append(reply);
        }
        bubble.append(readable(p.text, `board-message:${m.id}`));
        body.append(bubble);
        p.references.forEach((r) =>
          body.append(
            el(
              "span",
              `${r.repository}@${short(r.commit)} · ${r.path}:${r.start_line}–${r.end_line} · SHA-256 ${r.sha256}`,
              "reference",
            ),
          ),
        );
        if (item.record.origin.state === "revoked")
          body.append(
            el("span", `撤销：${item.record.origin.reason}`, "reference"),
          );
        if (byId.has(j.id)) {
          const process = el(
            "button",
            "查看发言者的执行过程 →",
            "thread-action",
          );
          process.addEventListener("click", () => choose(j.id, true));
          body.append(process);
        }
      }
      row.append(body);
      root.append(row);
    }
  }
  $("process-count").textContent = list.filter((j) => session(j)).length || "";
  if (changedView)
    root.scrollTop = chatMode === "process" ? 0 : root.scrollHeight;
  else if (bottom) root.scrollTop = root.scrollHeight;
  else root.scrollTop = scroll;
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
      chatMode = "public";
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
    text.append(el("small", `${j.model} · ${labels[j.state] || j.state}`));
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
  if (e.key !== "Escape") return;
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
$("public-chat").addEventListener("click", () => {
  chatMode = "public";
  render();
});
$("session-chat").addEventListener("click", () => choose(selected, true));
setInterval(connection, 1000);
setInterval(() => {
  if (!connected) refresh();
}, 10000);
refresh();
connect();
