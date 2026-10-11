import { avatar } from "./avatar.js";
import { busy } from "./feedback.js";
import { t } from "./i18n.js";
import { api, change, query } from "./api.js";
import { close, el, get, notice, open, presence, value } from "./dom.js";
import type { Page, Person } from "./types.js";
export const people = new Map<string, Person>();
export const display = (id: string): string =>
  id === "operator" ? t("people.e19796712f") : people.get(id)?.name || id;
let next: string | null = null,
  generation = 0;
let personId = "";
export async function loadPeople(append = false): Promise<void> {
  const request = ++generation;
  try {
    const page = await api<Page<Person>>(
      "/api/im/people?" +
        query({
          after: append ? next || "" : "",
          query: value("people-search"),
        }),
    );
    if (generation !== request) return;
    const root = get("people-list");
    if (!append) root.replaceChildren();
    next = page.next_after;
    get("more-people").hidden = !next;
    if (!root.childElementCount && !page.people.length) root.append(el("p", t("ux.peopleEmpty"), "empty"));
    for (const person of page.people) {
      people.set(person.id, person);
      const card = el("article", undefined, "card");
      const heading = el("header", undefined, "identity-heading");
      heading.append(avatar(person.id), el("h3", person.name));
      card.append(
        heading,
        el("code", person.id),
        el(
          "p",
          t("people.63daae1beb", {
            p0: presence(person.presence),
            p1: person.connections,
          }),
        ),
      );
      if (person.application_id)
        card.append(
          el("small", t("people.ad1028ba2a", { p0: person.application_id })),
        );
      if (person.id !== "operator") {
        const link = el("a", t("workspace.configureExecution"));
        link.href = "#settings/bindings";
        card.append(link);
      }
      root.append(card);
    }
  } catch (error) {
    notice(error);
  }
}
export function initPeople(): void {
  get("new-person").onclick = () => {
    personId = "agent-" + crypto.randomUUID();
    get<HTMLFormElement>("person-form").reset();
    notice("", "person-status");
    open("person-dialog");
  };
  get("search-people").onclick = () => void loadPeople();
  get("more-people").onclick = () => void loadPeople(true);
  get<HTMLFormElement>("person-form").onsubmit = async (event) => {
    event.preventDefault();
    const done = busy(get<HTMLFormElement>("person-form").querySelector<HTMLButtonElement>("button.primary")!);
    if (!done) return;
    try {
      await change({
        operation: "person",
        person: {
          id: personId,
          name: value("person-name"),
        },
      });
      close("person-dialog");
      await loadPeople();
    } catch (error) {
      notice(error, "person-status");
    } finally {
      done();
    }
  };
}
