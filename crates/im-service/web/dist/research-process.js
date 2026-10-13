import { api, query } from "./api.js";
import { button, el } from "./dom.js";
import { label, t } from "./i18n.js";
// Retain only bounded expansion keys; attempts never share an expansion or page.
const expanded = new Set();
export function researchProcess(goal, work) {
    const key = JSON.stringify([goal.input.group_id, goal.input.id, work.id, work.attempt]);
    const details = el("details", undefined, "research-process");
    details.append(el("summary", t("researchProcess.open")));
    const content = el("div"), controls = el("div", undefined, "card-actions");
    let next = null, loaded = false, loading = false;
    async function load(before) {
        if (loading)
            return;
        loading = true;
        content.replaceChildren(el("p", t("researchProcess.loading")));
        try {
            const page = await api("/api/research-process?" + query({
                group_id: goal.input.group_id, goal_id: goal.input.id,
                work_id: work.id, attempt: work.attempt,
                ...(before === undefined ? {} : { before }),
            }));
            if (!details.isConnected)
                return;
            if (page.goal_revision !== goal.revision || page.group_id !== goal.input.group_id || page.goal_id !== goal.input.id || page.work_id !== work.id || page.attempt !== work.attempt)
                throw new Error(t("researchProcess.unavailable"));
            next = page.next_before;
            older.disabled = next === null;
            const list = el("ol");
            for (const event of page.events) {
                const row = el("li");
                row.append(el("strong", label("SessionEventKind", event.title)), el("small", label("SessionItemStatus", event.status)));
                for (const text of [event.text, event.command, event.output])
                    if (text)
                        row.append(el("pre", text));
                list.append(row);
            }
            content.replaceChildren(el("p", t("researchProcess.scope")), page.events.length ? list : el("p", t("researchProcess.empty")));
            loaded = true;
        }
        catch {
            content.replaceChildren(el("p", t("researchProcess.unavailable")));
        }
        finally {
            loading = false;
        }
    }
    const refresh = button(t("researchProcess.refresh"), () => load());
    const older = button(t("researchProcess.older"), () => next === null ? undefined : load(next));
    older.disabled = true;
    controls.append(refresh, older);
    details.append(controls, content);
    details.ontoggle = () => {
        if (details.open) {
            if (expanded.size >= 64)
                expanded.delete(expanded.values().next().value);
            expanded.add(key);
            if (!loaded)
                void load();
        }
        else
            expanded.delete(key);
    };
    details.open = expanded.has(key);
    return details;
}
