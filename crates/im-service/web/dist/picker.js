import { t } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, notice } from "./dom.js";
import { people } from "./people.js";
export class Picker {
    chosen = new Map();
    after = null;
    generation = 0;
    search = el("input");
    list = el("div", undefined, "picker-list");
    selection = el("div", undefined, "selection");
    more = button(t("picker.655bddfaf7"), () => void this.load(true));
    excluded = new Set(["operator"]);
    constructor(root) {
        this.search.placeholder = t("picker.d2326e7cf5");
        this.search.setAttribute("aria-label", t("picker.d2326e7cf5"));
        const row = el("div", undefined, "search-row");
        row.append(this.search, button(t("picker.3003318e72"), () => void this.load()));
        this.search.onkeydown = (event) => {
            if (event.key === "Enter") {
                event.preventDefault();
                void this.load();
            }
        };
        root.append(row, this.selection, this.list, this.more);
    }
    reset(excluded = ["operator"]) {
        this.generation++;
        this.chosen.clear();
        this.excluded = new Set(excluded);
        this.search.value = "";
        this.selection.replaceChildren();
        void this.load();
    }
    async load(append = false) {
        const generation = ++this.generation;
        try {
            const page = await api("/api/im/people?" +
                query({
                    after: append ? this.after || "" : "",
                    query: this.search.value,
                }));
            if (generation !== this.generation)
                return;
            if (!append)
                this.list.replaceChildren();
            this.after = page.next_after;
            this.more.hidden = !this.after;
            for (const person of page.people.filter((p) => !this.excluded.has(p.id))) {
                people.set(person.id, person);
                const label = el("label", undefined, "picker-person"), check = el("input");
                check.type = "checkbox";
                check.checked = this.chosen.has(person.id);
                check.onchange = () => {
                    if (check.checked)
                        this.chosen.set(person.id, person);
                    else
                        this.chosen.delete(person.id);
                    this.selection.textContent = t("picker.980bd2c7a9", {
                        p0: this.chosen.size,
                        p1: [...this.chosen.values()].map((p) => p.name).join("、"),
                    });
                };
                label.append(check, el("span", person.name), el("small", person.id));
                this.list.append(label);
            }
            if (!this.list.childElementCount)
                this.list.append(el("p", t("picker.840deee408"), "empty"));
        }
        catch (error) {
            notice(error);
        }
    }
}
