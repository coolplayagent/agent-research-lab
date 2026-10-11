import { avatar } from "./avatar.js";
import { api, query } from "./api.js";
import { el, get, notice } from "./dom.js";
import { t } from "./i18n.js";
import { people } from "./people.js";
import { mentionText, resolveMentions, mentionNames } from "./mention-model.js";
import type { Person, GroupView, Page } from "./types.js";
import type { Scenario } from "./goal-types.js";
import type { ServiceView } from "./service-types.js";
let selected = "", generation = 0;
let members: Person[] = [], ready: Promise<void> = Promise.resolve();
let choices: Person[] = [], highlighted = 0;
export function selectMentions(groupId: string): void {
  selected = groupId;
  const request = ++generation;
  members = []; closeMentions();
  ready = (async () => {
    if (!groupId) return;
    const ids = new Set<string>();
    let cursor = "";
    do {
      const page = await api<GroupView>("/api/crystal/view?" + query({ group_id: groupId, after: cursor }));
      if (request !== generation) return;
      page.members.members.forEach((member) => ids.add(member.person_id));
      const next = page.members.next_after || "";
      if (next === cursor) break;
      cursor = next;
    } while (cursor && ids.size < 10000);
    cursor = "";
    do {
      const page = await api<Page<Person>>("/api/im/people?" + query({ after: cursor }));
      if (request !== generation) return;
      page.people.forEach((person) => people.set(person.id, person));
      const next = page.next_after || "";
      if (next === cursor) break;
      cursor = next;
    } while (cursor && [...ids].some((id) => id !== "operator" && !people.has(id)));
    members = [...ids].filter((id) => id !== "operator").flatMap((id) => people.has(id) ? [people.get(id)!] : []);
    renderMentions();
  })();
  void ready.catch((error) => { if (request === generation) notice(error); });
}
function closeMentions(): void { get("mention-menu").hidden = true; choices = []; }
function fragment(): RegExpMatchArray | null {
  const input = get<HTMLTextAreaElement>("message-text");
  return input.value.slice(0, input.selectionStart).match(/(?:^|\s)@([^@\s]*)$/u);
}
export function insertMention(id: string): void {
  const person = people.get(id);
  if (!person || id === "operator") return;
  const input = get<HTMLTextAreaElement>("message-text"), match = fragment();
  const end = input.selectionStart, start = match ? end - match[1].length - 1 : end;
  const prefix = start > 0 && !/\s/u.test(input.value[start - 1]) ? " " : "";
  input.setRangeText(prefix + mentionText(person) + " ", start, end, "end");
  closeMentions(); input.focus(); input.dispatchEvent(new Event("input"));
}
function renderMentions(): void {
  const match = fragment(), menu = get("mention-menu");
  menu.replaceChildren();
  if (!match) { closeMentions(); return; }
  choices = members.filter((person) => person.name.toLocaleLowerCase().includes(match[1].toLocaleLowerCase())).slice(0, 8);
  highlighted = Math.min(highlighted, Math.max(0, choices.length - 1));
  menu.hidden = !choices.length;
  choices.forEach((person, index) => {
    const option = el("button");
    option.append(avatar(person.id), el("span", `@${person.name}`));
    option.type = "button";
    option.setAttribute("role", "option");
    option.setAttribute("aria-selected", String(index === highlighted));
    option.onmousedown = (event) => event.preventDefault();
    option.onclick = () => insertMention(person.id);
    menu.append(option);
  });
}
export function initMentions(): void {
  const input = get<HTMLTextAreaElement>("message-text");
  input.addEventListener("input", () => { highlighted = 0; renderMentions(); });
  input.addEventListener("keydown", (event) => {
    if (!choices.length || event.isComposing) return;
    if (["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(event.key) && !event.ctrlKey && !event.metaKey) {
      event.preventDefault(); event.stopImmediatePropagation();
      if (event.key === "Escape") closeMentions();
      else if (["Enter", "Tab"].includes(event.key)) insertMention(choices[highlighted].id);
      else { highlighted = (highlighted + (event.key === "ArrowDown" ? 1 : choices.length - 1)) % choices.length; renderMentions(); }
    }
  });
}
export async function prepareDiscussion(groupId: string, text: string): Promise<Record<string, unknown> | null> {
  if (!mentionNames(text).length) return null;
  const context = generation;
  await ready;
  if (selected !== groupId || context !== generation) throw new Error(t("mentions.changed"));
  const resolved = resolveMentions(text, members);
  if (resolved.unresolved.length) throw new Error(t("mentions.unresolved", { names: resolved.unresolved.map((name) => `@${name}`).join("、") }));
  const result = await api<{ scenarios: Scenario[] }>("/api/im/scenarios");
  const research = resolved.people.every((person) => person.application_id === "research");
  const scenario = result.scenarios.find((item) => item.id === (research ? "research" : "collaboration"));
  if (!scenario?.conversation) throw new Error(t("mentions.unavailable"));
  if (!research) {
    const { configuration } = await api<ServiceView>("/api/im/services");
    const missing = resolved.people.filter((person) => !configuration.adapters.some((adapter) => adapter.id === configuration.bindings[person.id] && adapter.enabled));
    if (missing.length) throw new Error(t("mentions.unbound", { names: missing.map((person) => `@${person.name}`).join("、") }));
  }
  return { id: "assigned-by-server", group_id: groupId, title: text.slice(0, 70), objective: text, acceptance: t("discussion.acceptance"), scenario: scenario.id, parameters: scenario.conversation.parameters, max_seconds: scenario.conversation.max_seconds, assignments: resolved.people.map((person) => ({ person_id: person.id, instruction: t("mentions.assignment", { name: person.name }) })) };
}
