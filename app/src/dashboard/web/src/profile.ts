import { workspaceState } from "./state.js";
import { $, el, badge, short, time } from "./ui.js";
import {
  session,
  agentPresence,
  name,
  handle,
  presenceLabel,
  avatar,
  choose,
  loadProfile,
  readable,
  role,
  fold,
} from "./identity.js";
import { dismissProfile, setFollowing } from "./preferences.js";
import { openPerson } from "./people-profile.js";
export function details(j?: any): any {
  const root = $("detail"),
    s = j && session(j),
    profile = j && workspaceState.profiles.get(j.run_id);
  const key = JSON.stringify([j, s?.bytes, profile, j && agentPresence(j)]);
  if (key === workspaceState.detailKey) return;
  workspaceState.detailKey = key;
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
  title.append(
    el("span", presenceLabel(j), `im-status ${agentPresence(j).state}`),
  );
  hero.append(avatar(j), title);
  root.append(hero, el("p", `${j.model} · ${j.backend}`, "persona-model"));
  if (agentPresence(j).pending)
    root.append(
      el(
        "p",
        `${agentPresence(j).pending} 条消息待投递；在线后继续接收。`,
        "process-meta",
      ),
    );
  const inspect = el("button", "打开会话历史", "inspect-process");
  inspect.addEventListener("click", () => {
    dismissProfile();
    if (j.cohort_id !== workspaceState.room) {
      workspaceState.room = j.cohort_id || "";
      setFollowing(false);
    }
    $("search").value = "";
    $("state").value = "";
    choose(j.id, true);
  });
  root.append(inspect, el("h3", "Soul · 角色准则"));
  if (!workspaceState.profiles.has(j.run_id)) loadProfile(j);
  if (j.profile?.person_id) {
    const personButton = el(
      "button",
      "数字人档案 · 全部会话与记忆",
      "inspect-process",
    );
    personButton.addEventListener("click", () =>
      openPerson(j.profile.person_id),
    );
    root.append(personButton);
    if (j.persona?.soul)
      root.append(
        el("h3", `Soul · 版本 ${j.persona.revision}`),
        readable(j.persona.soul, `persona-soul:${j.run_id}`, "soul-text"),
      );
  }
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
      workspaceState.profiles.delete(j.run_id);
      workspaceState.detailKey = "";
      details(j);
    });
    root.append(retry);
  }
  root.append(
    el(
      "p",
      "任务角色准则与独立 Soul 分别保留；任务使用创建时固定的身份、Soul 和记忆上下文。",
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
    ["执行技术", j.backend],
    ["工作范围", `${j.repository} · ${j.write ? "候选写入" : "只读研究"}`],
    [
      "记忆",
      j.persona?.memory
        ? "relay-memory · 已固定上下文"
        : j.profile?.use_memory
          ? "本任务隔离记忆"
          : "历史任务未注入数字人记忆",
    ],
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
  const records = workspaceState.data.boards.flatMap(
      (b?: any) => b.retained_messages || [],
    ),
    sent = records.filter((m?: any) => m.message.task_id === j.id),
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
    ["数字人记忆 SHA-256", j.persona?.memory?.sha256],
  ]) {
    if (!v) continue;
    const b = el("div", undefined, "binding");
    b.append(el("span", k), el("code", v));
    evidence.append(b);
  }
  ids.forEach((id?: any) => evidence.append(el("p", id)));
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
