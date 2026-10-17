import { OutcomeKind, TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import { el, button, actions } from "./dom.js";
import { input, select, field } from "./forms.js";
import { publicationForm, pathForm } from "./outcome-forms.js";
import { outcomePaths, outcomeLink } from "./outcome-detail.js";
import type { Topic } from "./research-types.js";
import type { OutcomePage, OutcomePath } from "./outcome-types.js";
export function outcomeBoard(topic: Topic): HTMLElement {
  const panel = el("section", undefined, "outcome-board"), search = input(""), kind = select([["", t("outcome.allKinds")], ...Object.values(OutcomeKind).map(k => [k, label("OutcomeKind", k)] as [string, string])], ""),
    path = select([["", t("outcome.allPaths")]], ""), list = el("div", undefined, "outcome-list"), status = el("p");
  let after: number | null = null, snapshot: number | undefined, generation = 0, paths: OutcomePath[] = [];
  status.setAttribute("role", "status");
  const load = async (more = false) => {
    const request = ++generation; status.textContent = t("ux.loading");
    try {
      const page = await api<OutcomePage>("/api/v1/collaboration/outcomes?" + query({ topic_id: topic.id, query: search.value, ...(kind.value ? { kind: kind.value } : {}), goal_id: path.value, after: more ? String(after || 0) : "0", ...(more && snapshot !== undefined ? { snapshot: String(snapshot) } : {}) }));
      if (request !== generation || !panel.isConnected) return;
      if (!more) list.replaceChildren();
      for (const row of page.outcomes) {
        const card = el("article", undefined, "card"), heading = el("h4"); heading.append(outcomeLink(row.id, row.title));
        card.append(heading, el("p", `${label("OutcomeKind", row.kind)} · ${paths.find(p => p.goal_id === row.goal_id)?.name || row.goal_id}`), el("p", row.summary), el("p", [label("OutcomePublicationState", row.publication), label("OutcomeVerificationState", row.verification), label("OutcomeAcceptanceState", row.acceptance)].join(" · ")), el("p", t("outcome.referenceCount", { count: row.reference_count }))); list.append(card);
      }
      after = page.next_after; snapshot = page.snapshot; next.hidden = after === null;
      status.textContent = !page.outcomes.length && !more ? t("outcome.empty") : t("outcome.discoveryNote");
    } catch (error) { if (request === generation) status.textContent = String(error); }
  };
  const loadPaths = async () => { paths = await outcomePaths(topic.id); const current = path.value; path.replaceChildren(...select([["", t("outcome.allPaths")], ...paths.map(p => [p.goal_id, p.name] as [string, string])], current).children); path.value = current; await load(); };
  const next = button(t("outcome.more"), () => load(true)), publish = button(t("outcome.publish"), () => publicationForm(topic, paths, loadPaths)), link = button(t("outcome.pathForm"), () => pathForm(topic, loadPaths));
  publish.disabled = link.disabled = topic.state === TopicState.Archived; next.hidden = true;
  panel.append(el("h3", t("outcome.board")), el("p", t("outcome.boardNote")), actions(link, publish), field(t("outcome.search"), search), field(t("outcome.kind"), kind), field(t("outcome.path"), path), button(t("outcome.search"), () => load()), status, list, next);
  queueMicrotask(() => { void loadPaths().catch(error => { status.textContent = String(error); }); });
  return panel;
}
