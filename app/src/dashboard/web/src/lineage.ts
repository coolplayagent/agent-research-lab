import { workspaceState } from "./state.js";
import { $, el, short } from "./ui.js";
import { renderSystemMessages } from "./navigation.js";
export async function loadLineage(after: any = ""): Promise<any> {
  if (workspaceState.lineageLoading) return;
  workspaceState.lineageLoading = true;
  try {
    const response = await fetch(
      `/api/evolution/lineage${after ? `?after=${encodeURIComponent(after)}` : ""}`,
      { cache: "no-store", signal: AbortSignal.timeout(8000) },
    );
    if (!response.ok) throw Error("谱系证据无法读取或未通过完整性核验。");
    const value = await response.json(),
      root = $("lineage-nodes"),
      budget = $("lineage-budget");
    root.replaceChildren();
    budget.replaceChildren();
    workspaceState.lineageNext = value.next_after;
    $("lineage-next").hidden = !workspaceState.lineageNext;
    if (!value.available) {
      $("lineage-status").textContent = value.notice;
      return;
    }
    const report = value.report;
    $("lineage-status").textContent =
      `${report.task_kind} · ${value.total} 个策略版本 · 开发集谱系观察`;
    for (const [label, text] of [
      ["已评估", report.completed_evaluations],
      ["评估中", report.pending_evaluations],
      ["变异中", report.pending_expansions],
      ["剩余评估预算", report.budget_remaining],
    ])
      budget.append(el("dt", label), el("dd", text));
    for (const node of report.nodes) {
      const card = el("article", undefined, "lineage-node");
      card.append(
        el("h3", node.change_reason || short(node.version)),
        el("small", `${short(node.version, 12)} · ${node.role}`),
      );
      const facts = el(
        "p",
        `自身 ${node.successes}/${node.trials} 次成功 · 谱系 ${node.clade_successes}/${node.clade_trials} 次成功 · ${node.descendant_versions} 个后代`,
      );
      card.append(facts);
      if (node.parents.length)
        card.append(
          el(
            "p",
            `源自 ${node.parents.map((id?: any) => short(id, 12)).join("、")}`,
          ),
        );
      card.append(
        el(
          "p",
          node.clade_success_rate === null
            ? "证据尚不足"
            : `谱系成功率 ${(node.clade_success_rate * 100).toFixed(1)}% · 描述性后验均值 ${(node.clade_posterior_mean * 100).toFixed(1)}%`,
        ),
      );
      const evidence = el("details"),
        summary = el("summary", "版本与证据");
      evidence.append(summary);
      const source = node.source;
      for (const [label, text] of [
        ["代码", source?.source_commit],
        ["策略", node.version],
        ["策略门禁", report.policy_version],
        ["SuperPOD", report.superpod_commit],
        ["证据摘要", report.evidence_digest],
        ["耗时", `${node.duration_ms} ms`],
        ["Token", node.tokens ?? "尚未提供"],
      ]) {
        evidence.append(el("p", `${label}：${text || "尚未绑定"}`));
      }
      card.append(evidence);
      root.append(card);
    }
    workspaceState.serviceAlerts.delete("lineage");
  } catch (e) {
    workspaceState.serviceAlerts.set("lineage", e.message);
    renderSystemMessages();
  } finally {
    workspaceState.lineageLoading = false;
  }
}
export function observeLineage(value?: any): any {
  if (value !== workspaceState.lineageRevision) {
    workspaceState.lineageRevision = value;
    loadLineage();
  }
}
