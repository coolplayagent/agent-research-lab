import { t } from "./i18n.js";
import { api, change, query } from "./api.js";
import { button, close, el, get, notice, open, presence, value, } from "./dom.js";
export const people = new Map();
export const display = (id) => id === "operator" ? t("people.e19796712f") : people.get(id)?.name || id;
let next = null, generation = 0;
export async function loadPeople(append = false) {
    const request = ++generation;
    try {
        const page = await api("/api/im/people?" +
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
        for (const person of page.people) {
            people.set(person.id, person);
            const card = el("article", undefined, "card");
            card.append(el("h3", person.name), el("code", person.id), el("p", t("people.63daae1beb", {
                p0: presence(person.presence),
                p1: person.connections,
            })));
            if (person.application_id)
                card.append(el("small", t("people.ad1028ba2a", { p0: person.application_id })));
            if (person.id !== "operator")
                card.append(button(t("people.529a88ccb5"), () => void grant(person.id)));
            root.append(card);
        }
    }
    catch (error) {
        notice(error);
    }
}
export async function grant(person_id) {
    try {
        const result = await change({
            operation: "grant",
            person_id,
            lifetime_seconds: 86400,
        });
        get("credential-value").value = result.token;
        open("credential-dialog");
    }
    catch (error) {
        notice(error);
    }
}
export function initPeople() {
    get("new-person").onclick = () => {
        get("person-form").reset();
        notice("", "person-status");
        open("person-dialog");
    };
    get("search-people").onclick = () => void loadPeople();
    get("more-people").onclick = () => void loadPeople(true);
    get("person-form").onsubmit = async (event) => {
        event.preventDefault();
        try {
            await change({
                operation: "person",
                person: { id: value("person-id"), name: value("person-name") },
            });
            close("person-dialog");
            await loadPeople();
        }
        catch (error) {
            notice(error, "person-status");
        }
    };
}
