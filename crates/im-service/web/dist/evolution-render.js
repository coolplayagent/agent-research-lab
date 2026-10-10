import { t, label } from "./i18n.js";
import { el, button, get } from "./dom.js";
import { openGroup as selectConversation } from "./conversation.js";
const ns = "http://www.w3.org/2000/svg";
function svg(tag, attrs) {
    const node = document.createElementNS(ns, tag);
    for (const [k, v] of Object.entries(attrs))
        node.setAttribute(k, v);
    return node;
}
export function renderGraph(rows, select) {
    const host = get("evolution-canvas");
    host.replaceChildren();
    if (!rows.length) {
        host.append(el("p", t("research.noNodes")));
        return;
    }
    const positions = new Map(), columns = new Map();
    for (const row of rows) {
        const generation = row.node.generation, index = columns.get(generation) || 0;
        columns.set(generation, index + 1);
        positions.set(row.node.input.id, {
            x: 28 + generation * 260,
            y: 50 + index * 110,
        });
    }
    const width = Math.max(720, Math.max(...columns.keys()) * 260 + 300), height = Math.max(270, Math.max(...columns.values()) * 110 + 65), canvas = svg("svg", {
        viewBox: `0 0 ${width} ${height}`,
        width: String(width),
        height: String(height),
        role: "img",
        "aria-label": t("research.graph"),
    });
    canvas.classList.add("evolution-svg");
    for (const generation of columns.keys()) {
        const text = svg("text", {
            x: String(28 + generation * 260),
            y: "24",
            class: "generation",
        });
        text.textContent = t("research.generation", { p0: generation });
        canvas.append(text);
    }
    for (const row of rows) {
        const point = positions.get(row.node.input.id);
        for (const parent of row.node.input.parents) {
            const start = positions.get(parent);
            if (!start)
                continue;
            const path = svg("path", {
                d: `M${start.x + 212},${start.y + 35} C${start.x + 238},${start.y + 35} ${point.x - 26},${point.y + 35} ${point.x},${point.y + 35}`,
                class: "evolution-edge",
            });
            canvas.append(path);
        }
    }
    for (const row of rows) {
        const point = positions.get(row.node.input.id), node = svg("g", {
            transform: `translate(${point.x},${point.y})`,
            tabindex: "0",
            role: "button",
            "aria-label": `${row.subject.name} ${row.node.input.version} ${label("EvolutionState", row.status)}`,
            "data-node-id": row.node.input.id,
            "data-state": row.status,
        });
        node.classList.add("evolution-node");
        node.append(svg("rect", { width: "212", height: "76", rx: "12" }));
        const title = svg("text", { x: "12", y: "23" }), subtitle = svg("text", { x: "12", y: "43", class: "evolution-version" }), status = svg("text", { x: "12", y: "63", class: "evolution-status" });
        title.textContent = row.subject.name.slice(0, 23);
        subtitle.textContent = row.node.input.version.slice(0, 27);
        status.textContent = `${label("EvolutionOperation", row.operation)} · ${label("EvolutionState", row.status)}`;
        node.append(title, subtitle, status);
        const activate = () => {
            canvas
                .querySelectorAll("[aria-pressed]")
                .forEach((n) => n.setAttribute("aria-pressed", "false"));
            node.setAttribute("aria-pressed", "true");
            select(row);
        };
        node.onclick = activate;
        node.onkeydown = (event) => {
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                activate();
            }
        };
        canvas.append(node);
    }
    host.append(canvas);
}
export function showNode(row) {
    const panel = get("evolution-detail");
    panel.hidden = false;
    panel.replaceChildren(el("h3", `${row.subject.name} · ${row.node.input.version}`), el("p", `${label("EvolutionState", row.status)} · ${label("EvolutionOperation", row.operation)}`), el("p", row.topic.title), el("code", row.node.input.id), el("p", t("research.reference")), el("code", row.node.input.reference));
    if (row.node.input.parents.length)
        panel.append(el("p", t("research.parents")), el("pre", row.node.input.parents.join("\n")));
    const pins = el("dl", undefined, "metrics");
    for (const [key, value] of Object.entries(row.node.input.pins)) {
        pins.append(el("dt", t("research.pin." + key)), el("dd", value || t("research.notPinned")));
    }
    panel.append(pins);
    if (row.goal) {
        panel.append(el("h4", row.goal.input.title), el("p", label("GoalState", row.goal.state)), el("p", row.goal.input.acceptance));
        for (const work of row.goal.work) {
            const card = el("article", undefined, "card");
            card.append(el("strong", `${work.person_id} · ${label("WorkState", work.state)}`));
            if (work.result) {
                card.append(el("p", work.result.code
                    ? label("WorkResultCode", work.result.code)
                    : work.result.summary));
                const details = el("details");
                details.append(el("summary", t("research.evidence")), el("pre", JSON.stringify(work.result.evidence, null, 2)));
                card.append(details);
            }
            panel.append(card);
        }
    }
    panel.append(el("p", t("research.evidenceNote")), button(t("research.openGroup"), () => {
        location.hash = "groups";
        void selectConversation(row.topic.group_id);
    }));
}
