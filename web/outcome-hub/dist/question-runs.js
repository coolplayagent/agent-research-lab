import { TaskKind } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, el, button } from "./dom.js";
import { t, label } from "./i18n.js";
let group = "", generation = 0, loading = false;
export function selectQuestionRuns(id) { group = id; generation++; get("question-runs").replaceChildren(); void refresh(); }
async function refresh() {
    if (!group || document.hidden || loading)
        return;
    loading = true;
    const selected = group, request = generation;
    try {
        const all = [];
        let after = "";
        do {
            const page = await api("/api/v2/collaboration/questions?" + query({ group_id: selected, after }));
            if (request !== generation)
                return;
            all.push(...page.tasks);
            after = page.next_after || "";
        } while (after && all.filter(t => t.kind !== TaskKind.Outcome).length < 3);
        const root = get("question-runs");
        root.replaceChildren();
        const rows = all.filter(task => task.kind !== TaskKind.Outcome).sort((a, b) => b.created_ms - a.created_ms).slice(0, 3);
        for (const task of rows) {
            const row = el("div", undefined, "question-run");
            row.append(el("span", t(task.kind === TaskKind.Question ? "tasks.directQuestion" : "tasks.routing")), el("span", task.error ? label("TaskErrorCode", task.error) : (task.message_processing ? label("MessageProcessingState", task.message_processing) : label("TaskState", task.state))), button(t("tasks.viewRun"), () => { location.hash = "groups/" + encodeURIComponent(selected) + "/tasks/" + encodeURIComponent(task.input.id); }));
            root.append(row);
        }
        root.hidden = !rows.length;
    }
    catch { /* The conversation connection indicator owns transport status. */ }
    finally {
        loading = false;
    }
}
setInterval(() => { void refresh(); }, 3000);
