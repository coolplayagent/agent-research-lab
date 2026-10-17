import { uuid } from "/assets/shared/identity.js";
import { avatar } from "./avatar.js";
import { busy } from "./feedback.js";
import { t } from "./i18n.js";
import { api, change, query } from "./api.js";
import { actions, button, close, el, get, notice, open, presence, value } from "./dom.js";
export const people = new Map();
export const display = (id) => id === "operator" ? t("people.e19796712f") : people.get(id)?.name || id;
let next = null, generation = 0;
let editing = null;
let personId = "";
export async function loadPeople(append = false) {
    const request = ++generation;
    try {
        const page = await api("/api/v1/collaboration/people?" +
            query({
                after: append ? next || "" : "",
                query: value("people-search"),
            }));
        if (generation !== request)
            return;
        const root = get("people-list");
        if (!append)
            root.replaceChildren();
        next = page.next_after;
        get("more-people").hidden = !next;
        if (!root.childElementCount && !page.people.length)
            root.append(el("p", t("ux.peopleEmpty"), "empty"));
        for (const person of page.people) {
            people.set(person.id, person);
            const card = el("article", undefined, "card");
            const heading = el("header", undefined, "identity-heading");
            heading.append(avatar(person.id), el("h3", person.name));
            card.append(heading, el("code", person.id), el("p", t("people.63daae1beb", {
                p0: presence(person.presence),
                p1: person.connections,
            })));
            if (person.application_id)
                card.append(el("small", t("people.ad1028ba2a", { p0: person.application_id })));
            if (person.id !== "operator") {
                const link = el("a", t("workspace.configureExecution"));
                link.href = "#settings/bindings";
                link.className = "button-link";
                card.append(actions(button(t("layout.editPerson"), () => openPerson(person)), link));
            }
            root.append(card);
        }
    }
    catch (error) {
        notice(error);
    }
}
function openPerson(person) {
    editing = person;
    personId = person?.id || "agent-" + uuid();
    get("person-form").reset();
    get("person-dialog-title").textContent = person ? t("layout.editPerson") : t("html.a29c23de8c");
    get("person-name").value = person?.name || "";
    notice("", "person-status");
    open("person-dialog");
}
export function initPeople() {
    get("new-person").onclick = () => openPerson(null);
    get("search-people").onclick = () => void loadPeople();
    get("more-people").onclick = () => void loadPeople(true);
    get("person-form").onsubmit = async (event) => {
        event.preventDefault();
        const done = busy(get("person-form").querySelector("button.primary"));
        if (!done)
            return;
        try {
            const name = value("person-name");
            await change({
                operation: "person",
                person: {
                    id: personId,
                    name,
                    application_id: editing?.application_id || null,
                },
            });
            const person = people.get(personId);
            if (person)
                people.set(personId, { ...person, name });
            const search = get("people-search");
            if (!name.toLocaleLowerCase().includes(search.value.toLocaleLowerCase()) && !personId.includes(search.value))
                search.value = "";
            close("person-dialog");
            await loadPeople();
            document.dispatchEvent(new Event("workspace-people"));
        }
        catch (error) {
            notice(error, "person-status");
        }
        finally {
            done();
        }
    };
}
