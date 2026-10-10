import { workspaceState } from "./state.js";
import { fold, readable, name } from "./identity.js";
import { el } from "./ui.js";
export function isError(e?: any): any {
  return (
    ["turn.failed", "error"].includes(e.type) ||
    ["failed", "error"].includes(e.status) ||
    (e.exit_code != null && e.exit_code !== 0)
  );
}
export function eventDetails(j?: any, e?: any): any {
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
export function processBlock(j?: any, s?: any): any {
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
    tools.forEach((e?: any) => group.append(eventDetails(j, e)));
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
  if (workspaceState.historyState?.following && j.report?.summary) {
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
