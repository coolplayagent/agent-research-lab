import { ExecutionState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { workspaceState } from "./state.js";
import { $, el, svg } from "./ui.js";
import { maximize, session, name, presenceLabel, agentPresence, roleLabel, choose, } from "./identity.js";
import { render } from "./observer.js";
export function graph(list) {
    const root = $("graph");
    root.replaceChildren();
    const ranked = [...list].sort((a, b) => Number(a.state !== ExecutionState.Running) -
        Number(b.state !== ExecutionState.Running));
    const shown = ranked.slice(0, 8);
    $("graph-limit").textContent =
        list.length > 8
            ? t("graph.5cd2dcf0e1", { p0: shown.length, p1: list.length })
            : t("graph.49f0499185");
    if (!shown.length) {
        root.append(el("div", t("graph.2af9ef0999"), "empty"));
        return;
    }
    const canvas = svg("svg", {
        viewBox: "0 0 800 490",
        role: "group",
        "aria-label": t("graph.9d6d143aaa"),
    }), defs = svg("defs");
    const glow = svg("radialGradient", {
        id: "crystal-fill",
        cx: "35%",
        cy: "25%",
        r: "75%",
    });
    [
        ["0%", "#fff"],
        ["45%", "#dbf7f4"],
        ["85%", "#b5e4e1"],
        ["100%", "#d9eff0"],
    ].forEach(([offset, color]) => glow.append(svg("stop", { offset, "stop-color": color })));
    defs.append(glow);
    for (const [id, color] of [
        ["pub", "#3faaa0"],
        ["ctx", "#79a2c3"],
    ]) {
        const m = svg("marker", {
            id,
            viewBox: "0 0 10 10",
            refX: 9,
            refY: 5,
            markerWidth: 5,
            markerHeight: 5,
            orient: "auto",
        });
        m.append(svg("path", { d: "M0 0 L10 5 L0 10 z", fill: color }));
        defs.append(m);
    }
    canvas.append(defs);
    canvas.append(svg("ellipse", {
        cx: 400,
        cy: 240,
        rx: 288,
        ry: 173,
        fill: "none",
        stroke: "#e2eeee",
        "stroke-dasharray": "3 7",
    }), svg("circle", {
        cx: 400,
        cy: 240,
        r: 104,
        fill: "none",
        stroke: "#deeeec",
    }));
    const board = workspaceState.data.boards.find((b) => b.cohort_id === workspaceState.room), allMessages = board?.retained_messages || [];
    const positions = [];
    shown.forEach((j, i) => {
        const angle = -Math.PI / 2 + (i * Math.PI * 2) / shown.length;
        const x = 400 + 286 * Math.cos(angle), y = 240 + 181 * Math.sin(angle);
        positions.push({ j, x, y, angle });
        const x1 = x - 62 * Math.cos(angle), y1 = y - 32 * Math.sin(angle), x2 = 400 + 88 * Math.cos(angle), y2 = 240 + 88 * Math.sin(angle);
        const path = `M${x1},${y1} L${x2},${y2}`;
        canvas.append(svg("path", { d: path, class: "membership" }));
        const count = allMessages.filter((m) => m.message.task_id === j.id).length;
        if (count)
            canvas.append(svg("path", { d: path, class: "publish", "marker-end": "url(#pub)" }));
        if (j.context?.message_ids?.length ||
            allMessages.some((m) => (m.message.proposal.recipients || []).includes(j.id)))
            canvas.append(svg("path", {
                d: `M${x2 + 4},${y2 + 4} L${x1 + 4},${y1 + 4}`,
                class: "context",
                "marker-end": "url(#ctx)",
            }));
        if ((workspaceState.publishPulses.get(j.id) || 0) > Date.now())
            canvas.append(svg("path", { d: path, class: "traffic" }));
    });
    const orb = svg("g", {
        class: "board",
        tabindex: 0,
        role: "button",
        "aria-label": t("graph.27a0a36784"),
    });
    orb.append(svg("circle", {
        cx: 400,
        cy: 240,
        r: 86,
        fill: "url(#crystal-fill)",
        stroke: "#97cfca",
        "stroke-width": 1,
    }), svg("ellipse", {
        cx: 382,
        cy: 205,
        rx: 37,
        ry: 20,
        fill: "#fff",
        opacity: 0.45,
        transform: "rotate(-25 382 205)",
    }), svg("text", { x: 400, y: 225 }, t("graph.a1208633c3")), svg("text", { x: 400, y: 247 }, t("graph.18b7db5492")), svg("text", { x: 400, y: 269, class: "board-sub" }, t("graph.2a1d5edd7a", {
        p0: allMessages.filter((m) => !(m.message.proposal.recipients || []).length).length,
        p1: allMessages.filter((m) => (m.message.proposal.recipients || []).length).length,
    })));
    const open = () => {
        if (workspaceState.focusPanel === "graph")
            maximize("chat");
        workspaceState.chatRoute = "";
        workspaceState.chatKey = "";
        render();
        $("timeline").scrollTop = $("timeline").scrollHeight;
    };
    orb.addEventListener("click", open);
    orb.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            open();
        }
    });
    canvas.append(orb);
    positions.forEach(({ j, x, y }) => {
        const s = session(j), event = s?.events?.at(-1);
        const busy = (workspaceState.pulses.get(j.run_id) || 0) > Date.now();
        const group = svg("g", {
            class: `node ${j.id === workspaceState.selected ? "selected" : ""} ${busy ? "busy" : ""}`,
            transform: `translate(${x - 78},${y - 32})`,
            tabindex: 0,
            role: "button",
            "aria-label": `${name(j)} ${presenceLabel(j)}`,
        });
        group.append(svg("title", {}, j.id), svg("rect", { width: 156, height: 64, rx: 8 }), svg("circle", {
            cx: 13,
            cy: 16,
            r: 3,
            fill: {
                online: "#199377",
                chatting: "#159aa4",
                busy: "#c68a3e",
                offline: "#a7b3b7",
            }[agentPresence(j).state],
        }), svg("text", { x: 23, y: 20 }, name(j)), svg("text", { x: 12, y: 37, class: "node-sub" }, `${roleLabel(j)} · ${j.model} · ${presenceLabel(j)}`), svg("text", { x: 12, y: 52, class: "node-activity" }, event?.title ||
            (j.state === ExecutionState.Ready
                ? t("graph.2436c2bf8c")
                : t("graph.cafc1c5ce1"))));
        const inspect = () => {
            choose(j.id, true);
        };
        group.addEventListener("click", inspect);
        group.addEventListener("keydown", (e) => {
            if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                inspect();
            }
        });
        canvas.append(group);
    });
    root.append(canvas);
}
