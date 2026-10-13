import { uuid } from "/assets/shared/identity.js";
import {
  TopicState,
  SubjectKind,
  GroupKind,
} from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import {
  element,
  select,
  input,
  area,
  dialog,
  finish,
  field,
} from "./forms.js";
import type { Directory, Group } from "./types.js";
import type { Goal } from "./goal-types.js";
import type {
  Topic,
  Subject,
  EvolutionInput,
  EvolutionRow,
} from "./research-types.js";
const save = (value: unknown) =>
  api("/api/im/research", value, "manage-research");
export async function topicForm(
  old: Topic | undefined,
  done: (topic: Topic) => Promise<void>,
): Promise<void> {
  const groups: Group[] = [];
  let cursor = "", count = 0;
  do {
    const page = await api<Directory>("/api/crystal/view?" + query({ after: cursor }));
    groups.push(...page.directory.groups.filter(g => !g.archived && g.kind !== GroupKind.Board));
    count += page.directory.groups.length;
    const next = page.directory.next_after || "";
    if (next && (next === cursor || count >= 8192)) throw new Error(t("workspace.topicLimit"));
    cursor = next;
  } while (cursor);
  const box = dialog(t("research.topicForm")),
    id = input(old?.id || "topic-" + uuid()),
    title = input(old?.title || ""),
    objective = area(old?.objective || ""),
    state = select(
      Object.values(TopicState).map((s) => [s, label("TopicState", s)]),
      old?.state || TopicState.Active,
    );
  const options: [string, string][] = groups.map(group => [group.id, group.title]);
  if (old && !groups.some(group => group.id === old.group_id)) options.push([old.group_id, old.group_id]);
  const group = select(options, old?.group_id || groups[0]?.id || "");
  group.disabled = !!old;
  id.readOnly = !!old;
  for (const node of [id, title, objective, group]) node.required = true;
  box.form.append(
    field(t("research.id"), id),
    field(t("research.title"), title),
    field(t("research.objective"), objective),
    field(t("research.group"), group),
    element("p", t("research.groupNote")),
    field(t("research.state"), state),
  );
  if (!groups.length && !old) {
    const note = element("p", t("layout.topicPrerequisite"));
    const link = element("a", t("workspace.conversations"));
    link.href = "#groups"; link.onclick = () => box.dialog.close();
    note.append(link); box.form.append(note);
  }
  finish(box, async () => {
    const updated = await api<Topic>("/api/im/research", {
      operation: "topic",
      topic: {
        id: id.value.trim(),
        title: title.value.trim(),
        objective: objective.value.trim(),
        group_id: group.value.trim(),
        state: state.value,
        revision: old?.revision || 0,
      },
    }, "manage-research");
    await done(updated);
  });
}
export function subjectForm(
  topic: Topic,
  old: Subject | undefined,
  done: () => Promise<void>,
): void {
  const box = dialog(t("research.subjectForm")),
    id = input(old?.id || "subject-" + uuid()),
    name = input(old?.name || ""),
    kind = select(
      Object.values(SubjectKind).map((s) => [s, label("SubjectKind", s)]),
      old?.kind || SubjectKind.Repository,
    ),
    reference = input(old?.reference || "");
  id.readOnly = !!old;
  for (const node of [id, name, reference]) node.required = true;
  box.form.append(
    element("p", topic.title),
    field(t("research.id"), id),
    field(t("research.name"), name),
    field(t("research.kind"), kind),
    field(t("research.reference"), reference),
    element("p", t("research.referenceNote")),
  );
  finish(box, async () => {
    await save({
      operation: "subject",
      subject: {
        id: id.value.trim(),
        topic_id: topic.id,
        name: name.value.trim(),
        kind: kind.value,
        reference: reference.value.trim(),
        revision: old?.revision || 0,
      },
    });
    await done();
  });
}
export async function nodeForm(
  subject: Subject,
  group: string,
  nodes: EvolutionRow[],
  done: () => Promise<void>,
): Promise<void> {
  const goals: Goal[] = [];
  let after = "";
  do {
    const page = await api<{ goals: Goal[]; next_after: string | null }>(
      "/api/im/goals?" + query({ group_id: group, after }),
    );
    goals.push(...page.goals);
    after = page.next_after || "";
  } while (after && goals.length < 4096);
  const box = dialog(t("research.nodeForm")),
    id = input("version-" + uuid()),
    version = input(""),
    reference = input(subject.reference),
    goal = select(
      [
        ["", t("research.unassessed")],
        ...goals.map((g) => [g.input.id, g.input.title] as [string, string]),
      ],
      "",
    ),
    parents = area("");
  for (const node of [id, version, reference]) node.required = true;
  const available = nodes.filter((n) => n.subject.id === subject.id);
  parents.placeholder = available
    .slice(-4)
    .map((n) => n.node.input.id)
    .join("\n");
  box.form.append(
    element("p", subject.name),
    field(t("research.id"), id),
    field(t("research.version"), version),
    field(t("research.reference"), reference),
    field(t("research.parents"), parents),
    element("p", t("research.parentsNote")),
    field(t("research.goal"), goal),
    element("p", t("research.evidenceNote")),
  );
  const pins = {
    source: input(""),
    prompt: input(""),
    policy: input(""),
    superpod: input(""),
  };
  for (const [key, node] of Object.entries(pins))
    box.form.append(field(t("research.pin." + key), node));
  finish(box, async () => {
    const input: EvolutionInput = {
      id: id.value.trim(),
      subject_id: subject.id,
      parents: parents.value
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean),
      version: version.value.trim(),
      reference: reference.value.trim(),
      goal_id: goal.value || null,
      pins: {
        source: pins.source.value.trim() || null,
        prompt: pins.prompt.value.trim() || null,
        policy: pins.policy.value.trim() || null,
        superpod: pins.superpod.value.trim() || null,
      },
    };
    await save({ operation: "node", node: input });
    await done();
  });
}
