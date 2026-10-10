import { workspaceState } from "./state.js";
import { $ } from "./ui.js";
import { maximize } from "./identity.js";
export function readPreferences() {
    try {
        const value = JSON.parse(localStorage.getItem("crystal-observer-preferences") || "{}");
        return {
            follow: value.follow !== false,
            motion: value.motion !== false,
            leftWidth: value.leftWidth,
            rightWidth: value.rightWidth,
            leftCollapsed: value.leftCollapsed === true,
            rightCollapsed: value.rightCollapsed === true,
            chatCollapsed: value.chatCollapsed === true,
            sectionWeights: Object.fromEntries(Object.entries({ rooms: 45, private: 20, people: 35 }).map(([id, fallback]) => {
                const weight = value.sectionWeights?.[id];
                return [
                    id,
                    Number.isFinite(weight) && weight > 0
                        ? Math.min(weight, 1000)
                        : fallback,
                ];
            })),
            pinnedRooms: Array.isArray(value.pinnedRooms)
                ? value.pinnedRooms
                    .filter((x) => typeof x === "string")
                    .slice(-1000)
                : [],
            collapsed: Array.isArray(value.collapsed)
                ? value.collapsed.filter((x) => ["rooms", "private", "people"].includes(x))
                : [],
        };
    }
    catch (_) {
        return {};
    }
}
export function savePreferences() {
    try {
        localStorage.setItem("crystal-observer-preferences", JSON.stringify(workspaceState.preferences));
    }
    catch (_) { }
}
export function setFollowing(value) {
    workspaceState.following = value;
    workspaceState.preferences.follow = value;
    savePreferences();
}
export function layoutWidths() {
    const clamp = (value, fallback, min, max) => Math.max(min, Math.min(max, Number.isFinite(value) ? value : fallback));
    const width = Math.min(window.innerWidth, 2000), docked = window.innerWidth > 1200 && !workspaceState.focusPanel, rightMinimum = docked && !workspaceState.preferences.rightCollapsed ? 240 : 0, leftMax = Math.min(420, Math.max(180, width - 450 - rightMinimum)), left = clamp(workspaceState.preferences.leftWidth, 230, 180, leftMax), rightMax = docked
        ? Math.min(520, width - 450 - (workspaceState.preferences.leftCollapsed ? 0 : left))
        : Math.min(520, Math.max(240, window.innerWidth - 24)), right = clamp(workspaceState.preferences.rightWidth, 305, 240, rightMax);
    return { left, right, leftMax, rightMax, docked };
}
export function applySidebarLayout() {
    const widths = layoutWidths(), leftOpen = !workspaceState.preferences.leftCollapsed && !workspaceState.focusPanel, rightOpen = !workspaceState.preferences.rightCollapsed &&
        (widths.docked || document.body.classList.contains("profile-open"));
    const shell = $("collaboration-page");
    shell.style.setProperty("--left-track", `${leftOpen ? widths.left : 0}px`);
    shell.style.setProperty("--right-track", `${rightOpen && widths.docked ? widths.right : 0}px`);
    shell.style.setProperty("--right-width", `${widths.right}px`);
    shell.style.setProperty("--right-edge", `${rightOpen ? widths.right : 0}px`);
    for (const side of ["left", "right"]) {
        const open = side === "left" ? leftOpen : rightOpen, toggle = $("toggle-" + side + "-sidebar"), label = side === "left" ? "左侧栏" : "右侧栏", handle = $("resize-" + side);
        $(side + "-sidebar").hidden = !open;
        toggle.setAttribute("aria-expanded", String(open));
        toggle.textContent = (side === "left") === open ? "‹" : "›";
        toggle.title = `${open ? "收起" : "展开"}${label}`;
        toggle.setAttribute("aria-label", toggle.title);
        handle.setAttribute("aria-valuemin", side === "left" ? "180" : "240");
        handle.setAttribute("aria-valuemax", String(widths[side + "Max"]));
        handle.setAttribute("aria-valuenow", String(Math.round(widths[side])));
        handle.setAttribute("aria-valuetext", `${Math.round(widths[side])} 像素`);
        handle.title = "拖动调整宽度；方向键微调，双击还原";
    }
}
export function dismissProfile(collapse = false) {
    document.body.classList.remove("profile-open");
    if (collapse) {
        workspaceState.preferences.rightCollapsed = true;
        savePreferences();
    }
    applySidebarLayout();
}
export function setChatCollapsed(collapsed) {
    const timeline = $("timeline"), scroll = timeline.scrollTop, wasCollapsed = document.body.classList.contains("chat-collapsed");
    workspaceState.preferences.chatCollapsed = collapsed;
    document.body.classList.toggle("chat-collapsed", collapsed);
    $("collapse-chat").textContent = collapsed ? "⌃ 展开" : "⌄ 收起";
    $("collapse-chat").setAttribute("aria-expanded", String(!collapsed));
    $("collapse-chat").setAttribute("aria-label", collapsed ? "展开协作群聊" : "向下收起协作群聊");
    if (!collapsed && wasCollapsed)
        requestAnimationFrame(() => {
            timeline.scrollTo({
                top: Number(timeline.dataset.foldScroll || 0),
                behavior: "instant",
            });
        });
    else if (collapsed)
        timeline.dataset.foldScroll = String(scroll);
    savePreferences();
}
export function initSidebarControls() {
    $("toggle-left-sidebar").addEventListener("click", () => {
        const open = $("left-sidebar").hidden;
        if (workspaceState.focusPanel)
            maximize(workspaceState.focusPanel);
        workspaceState.preferences.leftCollapsed = !open;
        savePreferences();
        applySidebarLayout();
    });
    $("toggle-right-sidebar").addEventListener("click", () => {
        if (!$("right-sidebar").hidden)
            dismissProfile(true);
        else {
            workspaceState.preferences.rightCollapsed = false;
            document.body.classList.add("profile-open");
            savePreferences();
            applySidebarLayout();
        }
    });
    for (const side of ["left", "right"]) {
        const handle = $("resize-" + side), key = side + "Width";
        let drag = null;
        const resize = (width) => {
            const limits = layoutWidths();
            workspaceState.preferences[key] = Math.round(Math.max(side === "left" ? 180 : 240, Math.min(limits[side + "Max"], width)));
            applySidebarLayout();
        };
        const finish = () => {
            if (!drag)
                return;
            drag = null;
            document.body.classList.remove("resizing-sidebar");
            savePreferences();
        };
        handle.addEventListener("pointerdown", (e) => {
            if (e.button !== 0)
                return;
            drag = {
                pointer: e.pointerId,
                x: e.clientX,
                width: layoutWidths()[side],
            };
            handle.setPointerCapture(e.pointerId);
            handle.focus({ preventScroll: true });
            document.body.classList.add("resizing-sidebar");
            e.preventDefault();
        });
        handle.addEventListener("pointermove", (e) => {
            if (drag?.pointer !== e.pointerId)
                return;
            resize(drag.width + (e.clientX - drag.x) * (side === "left" ? 1 : -1));
        });
        handle.addEventListener("pointerup", finish);
        handle.addEventListener("pointercancel", finish);
        handle.addEventListener("lostpointercapture", finish);
        handle.addEventListener("dblclick", () => {
            workspaceState.preferences[key] = side === "left" ? 230 : 305;
            applySidebarLayout();
            savePreferences();
        });
        handle.addEventListener("keydown", (e) => {
            if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key))
                return;
            e.preventDefault();
            const limits = layoutWidths(), step = e.shiftKey ? 40 : 10;
            resize(e.key === "Home"
                ? 0
                : e.key === "End"
                    ? limits[side + "Max"]
                    : limits[side] +
                        (e.key === "ArrowRight" ? step : -step) *
                            (side === "left" ? 1 : -1));
            savePreferences();
        });
    }
    window.addEventListener("resize", applySidebarLayout);
    $("collapse-chat").addEventListener("click", () => setChatCollapsed(!workspaceState.preferences.chatCollapsed));
    applySidebarLayout();
    setChatCollapsed(workspaceState.preferences.chatCollapsed === true);
}
