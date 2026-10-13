import { uuid } from "/assets/shared/identity.js";
import { ScenarioCapability, TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { button, el } from "./dom.js";
import { area, check, dialog, field, finish, input, select } from "./forms.js";
import { label, t } from "./i18n.js";
import { display } from "./people.js";
import type { Scenario } from "./goal-types.js";
import type { Group, GroupView } from "./types.js";
import type { GraphPage, SubjectRow, TopicRow } from "./research-types.js";
import type { IterationInput } from "./iteration-types.js";

function numberField(value: number, maximum: number): HTMLInputElement {
  const node = input(String(value), "number");
  node.required = true;
  node.min = "1";
  node.max = String(maximum);
  node.step = "1";
  return node;
}

export async function iterationForm(
  group: Group,
  save: (input: IterationInput) => Promise<void>,
): Promise<void> {
  const groupId = group.id;
  const [{ scenarios }, topics] = await Promise.all([
    api<{ scenarios: Scenario[] }>("/api/im/scenarios"),
    groupTopics(groupId),
  ]);
  const supported = scenarios.filter(s => s.protocol_version === 1 &&
    s.capabilities?.includes(ScenarioCapability.Execute));
  const box = dialog(t("iterations.create"));
  const title = input(""), objective = area(""), acceptance = area("");
  for (const node of [title, objective, acceptance]) node.required = true;
  title.maxLength = 220;
  objective.maxLength = 4096;
  acceptance.maxLength = 4096;
  const scenario = select(supported.map(s => [s.id, label("Scenario", s.id)]), supported[0]?.id || "");
  const topic = select(topics.map(r => [r.topic.id, r.topic.title]), topics[0]?.topic.id || "");
  const subject = select([], ""), baseline = select([], "");
  for (const node of [scenario, topic, subject, baseline]) node.required = true;
  const parameterPanel = el("div"), capabilityPanel = el("p"), scenarioNote = el("p");
  const rounds = numberField(3, 256), seconds = numberField(600, 43200), budget = numberField(1800, 11059200);
  seconds.min = "30";
  const assignees = el("div", undefined, "iteration-assignees");
  const choices = new Map<string, { selected: HTMLInputElement; instruction: HTMLTextAreaElement; role: HTMLSelectElement; roleField: HTMLElement }>();
  let membersAfter: string | null = null;
  const moreMembers = button(t("html.ae4ed171f5"), () => loadMembers(true));
  moreMembers.hidden = true;
  async function loadMembers(more = false): Promise<void> {
    const page = await api<GroupView>("/api/crystal/view?" + query({
      group_id: groupId,
      after: more ? membersAfter || "" : "",
    }));
    if (!more) { choices.clear(); assignees.replaceChildren(); }
    for (const member of page.members.members) {
      if (member.person_id === "operator" || choices.has(member.person_id)) continue;
      const selected = check(false), instruction = area("");
      instruction.maxLength = 1200;
      instruction.disabled = true;
      instruction.placeholder = t("iterations.assignmentHint");
      selected.onchange = () => {
        instruction.disabled = !selected.checked;
        instruction.required = selected.checked;
      };
      const row = el("div", undefined, "iteration-assignment"), role = select([], "");
      const roleField = field(t("iterations.role"), role);
      row.append(field(display(member.person_id), selected), roleField, field(t("iterations.assignment"), instruction));
      assignees.append(row);
      choices.set(member.person_id, { selected, instruction, role, roleField });
    }
    if (page.members.next_after && page.members.next_after === membersAfter)
      throw new Error(t("iterations.pageLimit"));
    membersAfter = page.members.next_after;
    moreMembers.hidden = !membersAfter;
    showRoles();
  }
  function showRoles(): void {
    const roles = supported.find(s => s.id === scenario.value)?.assignment_roles || [];
    for (const choice of choices.values()) {
      const previous = choice.role.value;
      choice.role.replaceChildren();
      choice.roleField.hidden = !roles.length;
      for (const role of roles) { const option = el("option", label("CodingTaskRole", role)); option.value = role; choice.role.append(option); }
      if (roles.some(role => role === previous)) choice.role.value = previous;
    }
  }
  function showParameters(): void {
    parameterPanel.replaceChildren();
    const selected = supported.find(s => s.id === scenario.value);
    scenarioNote.textContent = selected ? label("ScenarioDescription", selected.id) : "";
    if (selected?.execution_seconds) {
      seconds.value = String(selected.execution_seconds);
      budget.value = String(selected.execution_seconds * Number(rounds.value));
    }
    showRoles();
    capabilityPanel.textContent = t("iterations.capabilities", {
      capabilities: (selected?.capabilities || []).map(c => label("ScenarioCapability", c)).join(" · "),
    });
    for (const item of selected?.fields || []) {
      const control = select(item.options.map(option => [option, item.id === "interaction"
        ? label("ResearchInteraction", option) : option]), item.options[0] || "");
      control.dataset.parameter = item.id;
      parameterPanel.append(field(label("ScenarioField", item.id), control));
    }
  }
  let selection = 0;
  async function loadBaseline(): Promise<void> {
    const request = ++selection, selectedSubject = subject.value, selectedTopic = topic.value;
    baseline.replaceChildren();
    baseline.disabled = true;
    if (!selectedSubject) return;
    const versions: { id: string; title: string }[] = [];
    let after = 0;
    do {
      const page = await api<GraphPage>("/api/im/evolution?" + query({
        topic_id: selectedTopic, subject_id: selectedSubject, after,
      }));
      if (request !== selection) return;
      versions.push(...page.nodes.map(row => ({ id: row.node.input.id, title: row.node.input.version })));
      if (page.next_after && (page.next_after <= after || versions.length >= 8192))
        throw new Error(t("iterations.pageLimit"));
      after = page.next_after || 0;
    } while (after);
    for (const version of versions) {
      const option = el("option", version.title);
      option.value = version.id;
      baseline.append(option);
    }
    baseline.disabled = !versions.length;
    if (!versions.length) box.status.textContent = t("iterations.needBaseline");
  }
  async function loadSubjects(): Promise<void> {
    const request = ++selection, selectedTopic = topic.value;
    subject.replaceChildren();
    baseline.replaceChildren();
    baseline.disabled = true;
    if (!selectedTopic) return;
    const subjects: SubjectRow[] = [];
    let after = "";
    do {
      const page = await api<{ subjects: SubjectRow[]; next_after: string | null }>(
        "/api/im/subjects?" + query({ topic_id: selectedTopic, after }),
      );
      if (request !== selection) return;
      subjects.push(...page.subjects);
      if (page.next_after && (page.next_after === after || subjects.length >= 8192))
        throw new Error(t("iterations.pageLimit"));
      after = page.next_after || "";
    } while (after);
    for (const row of subjects) {
      const option = el("option", row.subject.name);
      option.value = row.subject.id;
      subject.append(option);
    }
    await loadBaseline();
  }
  const showError = (error: unknown) => { box.status.textContent = String(error); };
  scenario.onchange = showParameters;
  topic.onchange = () => void loadSubjects().catch(showError);
  subject.onchange = () => void loadBaseline().catch(showError);
  box.form.append(
    el("p", group.title),
    field(t("iterations.scenario"), scenario), scenarioNote, capabilityPanel, parameterPanel,
    field(t("research.topics"), topic), field(t("iterations.subject"), subject),
    field(t("iterations.baseline"), baseline),
    field(t("research.title"), title), field(t("research.objective"), objective),
    field(t("iterations.acceptance"), acceptance),
    field(t("iterations.maxRounds"), rounds), field(t("iterations.maxSeconds"), seconds),
    field(t("iterations.budget"), budget),
    el("h3", t("iterations.participants")), el("p", t("iterations.independentReview")),
    assignees, moreMembers,
  );
  if (!topics.length || !supported.length) {
    box.form.append(el("p", t("iterations.prerequisite")));
    const link = el("a", t("research.topics"));
    link.href = "#topics";
    link.onclick = () => box.dialog.close();
    box.form.append(link);
  }
  const id = "iteration-" + uuid();
  showParameters();
  finish(box, async () => {
    const assignments = [...choices].filter(([, row]) => row.selected.checked)
      .map(([person_id, row]) => ({ person_id, instruction: row.instruction.value.trim() }));
    if (!assignments.length) throw new Error(t("goal-form.cf73f0c9c7"));
    if (!subject.value || !baseline.value) throw new Error(t("iterations.needBaseline"));
    if (Number(budget.value) < Number(seconds.value) || Number(budget.value) > Number(rounds.value) * Number(seconds.value))
      throw new Error(t("iterations.budgetInvalid"));
    const parameters: Record<string, unknown> = Object.fromEntries([...parameterPanel.querySelectorAll<HTMLSelectElement>("[data-parameter]")]
      .map(node => [node.dataset.parameter!, node.value]));
    if (supported.find(s => s.id === scenario.value)?.assignment_roles?.length) {
      parameters.roles = Object.fromEntries([...choices].filter(([, row]) => row.selected.checked).map(([id, row]) => [id, row.role.value]));
    }
    await save({
      id, group_id: groupId, scenario_id: scenario.value,
      subject_id: subject.value, baseline_version_id: baseline.value,
      title: title.value.trim(), objective: objective.value.trim(), acceptance: acceptance.value.trim(),
      assignments,
      parameters,
      max_rounds: Number(rounds.value), max_seconds: Number(seconds.value), total_budget_seconds: Number(budget.value),
    });
  });
  await Promise.all([loadMembers(), loadSubjects()]).catch(showError);
}

async function groupTopics(groupId: string): Promise<TopicRow[]> {
  const topics: TopicRow[] = [];
  let after = "", count = 0;
  do {
    const page = await api<{ topics: TopicRow[]; next_after: string | null }>(
      "/api/im/topics?" + query({ after }),
    );
    topics.push(...page.topics.filter(row => row.topic.group_id === groupId && row.topic.state === TopicState.Active));
    count += page.topics.length;
    if (page.next_after && (page.next_after === after || count >= 8192))
      throw new Error(t("iterations.pageLimit"));
    after = page.next_after || "";
  } while (after);
  return topics;
}
