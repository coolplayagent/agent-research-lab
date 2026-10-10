import { api, query } from "./api.js";
import { current } from "./conversation.js";
import { close, el, get, notice, open, value } from "./dom.js";
import { loadGoals } from "./goals.js";
import { display } from "./people.js";
let scenarios = [];
let groupId = "", goalId = "", next = null;
let selectedMembers = new Map();
function parameters() {
    const scenario = scenarios.find((s) => s.id === value("goal-scenario"));
    const root = get("goal-parameters");
    root.replaceChildren();
    get("goal-scenario-note").textContent = scenario?.description || "";
    for (const field of scenario?.fields || []) {
        const label = el("label", field.label), select = el("select");
        select.dataset.parameter = field.id;
        for (const choice of field.options) {
            const option = el("option", choice);
            option.value = choice;
            select.append(option);
        }
        label.append(select);
        root.append(label);
    }
}
async function members(append = false) {
    const selected = groupId;
    const page = await api("/api/crystal/view?" +
        query({ group_id: selected, after: append ? next || "" : "" }));
    if (groupId !== selected)
        return;
    const list = get("goal-assignees");
    if (!append) {
        list.replaceChildren();
        selectedMembers = new Map();
    }
    for (const person of page.members.members.filter((m) => m.person_id !== "operator")) {
        const label = el("label", undefined, "assignment-choice"), check = el("input"), instruction = el("textarea");
        check.type = "checkbox";
        check.setAttribute("aria-label", `指派 ${person.person_id}`);
        instruction.placeholder = "这位 Agent 的具体分工";
        instruction.setAttribute("aria-label", `${person.person_id} 的分工`);
        instruction.maxLength = 1200;
        instruction.rows = 2;
        instruction.disabled = true;
        check.onchange = () => {
            instruction.disabled = !check.checked;
            instruction.required = check.checked;
        };
        label.append(check, el("span", display(person.person_id)), instruction);
        list.append(label);
        selectedMembers.set(person.person_id, { check, instruction });
    }
    next = page.members.next_after;
    get("more-goal-members").hidden = !next;
}
export async function openGoal(scenarioId = "collaboration") {
    if (!current || current.kind === "board" || current.archived)
        return;
    groupId = current.id;
    goalId = "goal-" + crypto.randomUUID();
    get("goal-form").reset();
    get("goal-group-name").textContent = current.title;
    notice("", "goal-status");
    try {
        const result = await api("/api/im/scenarios");
        scenarios = result.scenarios;
        const select = get("goal-scenario");
        select.replaceChildren();
        for (const scenario of scenarios) {
            const option = el("option", scenario.name);
            option.value = scenario.id;
            select.append(option);
        }
        select.value = scenarios.some((s) => s.id === scenarioId)
            ? scenarioId
            : "collaboration";
        parameters();
        await members();
        open("goal-dialog");
    }
    catch (error) {
        notice(error);
    }
}
export function initGoals() {
    get("new-goal").onclick = () => void openGoal();
    get("goal-scenario").onchange = parameters;
    get("more-goal-members").onclick = () => {
        void members(true).catch((error) => notice(error, "goal-status"));
    };
    get("goal-form").onsubmit = async (event) => {
        event.preventDefault();
        const submit = get("goal-submit");
        submit.disabled = true;
        try {
            const assignments = [...selectedMembers]
                .filter(([, m]) => m.check.checked)
                .map(([person_id, m]) => ({
                person_id,
                instruction: m.instruction.value.trim(),
            }));
            if (!assignments.length)
                throw new Error("至少选择一位群成员并填写分工。");
            const parameters = Object.fromEntries([
                ...get("goal-parameters").querySelectorAll("[data-parameter]"),
            ].map((node) => [node.dataset.parameter, node.value]));
            await api("/api/im/goals", {
                operation: "create",
                goal: {
                    id: goalId,
                    group_id: groupId,
                    title: value("goal-title"),
                    objective: value("goal-objective"),
                    acceptance: value("goal-acceptance"),
                    scenario: value("goal-scenario"),
                    parameters,
                    assignments,
                    max_seconds: Number(value("goal-limit")),
                },
            });
            close("goal-dialog");
            await loadGoals();
            notice("目标已分配；Agent 提交结果后可在群内验收。");
        }
        catch (error) {
            notice(error, "goal-status");
        }
        finally {
            submit.disabled = false;
        }
    };
}
