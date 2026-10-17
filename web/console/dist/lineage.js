import { t } from "./i18n.js";
import { describe } from "./i18n.js";
import { workspaceState } from "./state.js";
import { $, el, short } from "./ui.js";
import { renderSystemMessages } from "./navigation.js";
export async function loadLineage(after = "") {
    if (workspaceState.lineageLoading)
        return;
    workspaceState.lineageLoading = true;
    try {
        const response = await fetch(`/api/evolution/lineage${after ? `?after=${encodeURIComponent(after)}` : ""}`, { cache: "no-store", signal: AbortSignal.timeout(8000) });
        if (!response.ok)
            throw Error(t("lineage.982c805a8a"));
        const value = await response.json(), root = $("lineage-nodes"), budget = $("lineage-budget");
        root.replaceChildren();
        budget.replaceChildren();
        workspaceState.lineageNext = value.next_after;
        $("lineage-next").hidden = !workspaceState.lineageNext;
        if (!value.available) {
            $("lineage-status").textContent = describe(value.notice);
            return;
        }
        const report = value.report;
        $("lineage-status").textContent = t("lineage.922c4c27c6", {
            p0: report.task_kind,
            p1: value.total,
        });
        for (const [label, text] of [
            [t("lineage.64363cae32"), report.completed_evaluations],
            [t("lineage.cab9bbad4e"), report.pending_evaluations],
            [t("lineage.67b8650b0d"), report.pending_expansions],
            [t("lineage.2b5dbc50a7"), report.budget_remaining],
        ])
            budget.append(el("dt", label), el("dd", text));
        for (const node of report.nodes) {
            const card = el("article", undefined, "lineage-node");
            card.append(el("h3", node.change_reason || short(node.version)), el("small", `${short(node.version, 12)} · ${node.role}`));
            const facts = el("p", t("lineage.ea8ca03f4f", {
                p0: node.successes,
                p1: node.trials,
                p2: node.clade_successes,
                p3: node.clade_trials,
                p4: node.descendant_versions,
            }));
            card.append(facts);
            if (node.parents.length)
                card.append(el("p", t("lineage.37c9259d7f", {
                    p0: node.parents.map((id) => short(id, 12)).join("、"),
                })));
            card.append(el("p", node.clade_success_rate === null
                ? t("lineage.3263fa1ad6")
                : t("lineage.e520e5112b", {
                    p0: (node.clade_success_rate * 100).toFixed(1),
                    p1: (node.clade_posterior_mean * 100).toFixed(1),
                })));
            const evidence = el("details"), summary = el("summary", t("lineage.6eae9eb694"));
            evidence.append(summary);
            const source = node.source;
            for (const [label, text] of [
                [t("lineage.e6f04ffbaa"), source?.source_commit],
                [t("lineage.9c8eb75c7e"), node.version],
                [t("lineage.8acca994c0"), report.policy_version],
                ["SuperPOD", report.superpod_commit],
                [t("lineage.4a98e96631"), report.evidence_digest],
                [t("lineage.e77e3d58b0"), `${node.duration_ms} ms`],
                ["Token", node.tokens ?? t("lineage.be4f923bcf")],
            ]) {
                evidence.append(el("p", `${label}：${text || t("lineage.1332a0b73a")}`));
            }
            card.append(evidence);
            root.append(card);
        }
        workspaceState.serviceAlerts.delete("lineage");
    }
    catch (e) {
        workspaceState.serviceAlerts.set("lineage", e.message);
        renderSystemMessages();
    }
    finally {
        workspaceState.lineageLoading = false;
    }
}
export function observeLineage(value) {
    if (value !== workspaceState.lineageRevision) {
        workspaceState.lineageRevision = value;
        loadLineage();
    }
}
