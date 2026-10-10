import { workspaceState } from "./state.js";
import { $ } from "./ui.js";
import { savePreferences } from "./preferences.js";
export function updateSectionSizing(): any {
  const ids = ["rooms", "private", "people"];
  workspaceState.preferences.sectionWeights ||= {
    rooms: 45,
    private: 20,
    people: 35,
  };
  for (const id of ids) {
    const section = $("section-" + id);
    section.style.setProperty(
      "--section-weight",
      workspaceState.preferences.sectionWeights[id],
    );
  }
  for (const id of ids.slice(0, 2)) {
    const handle = $("resize-section-" + id),
      section = $("section-" + id),
      next = ids
        .slice(ids.indexOf(id) + 1)
        .find((name?: any) => $("section-" + name).open);
    handle.hidden = !section.open || !next;
    if (handle.hidden) continue;
    handle.dataset.after = next;
    const height = section.getBoundingClientRect().height,
      other = $("section-" + next).getBoundingClientRect().height;
    handle.setAttribute("aria-controls", `section-${id} section-${next}`);
    handle.setAttribute("aria-valuemin", "72");
    handle.setAttribute(
      "aria-valuemax",
      String(Math.max(72, Math.round(height + other - 72))),
    );
    handle.setAttribute(
      "aria-valuenow",
      String(Math.max(72, Math.round(height))),
    );
    handle.setAttribute("aria-valuetext", `${Math.round(height)} 像素`);
    handle.title = "上下拖动调整分区高度；方向键微调，双击还原比例";
  }
}
export function initSectionSizing(): any {
  for (const id of ["rooms", "private"]) {
    const handle = $("resize-section-" + id);
    let drag = null;
    const pair = () => {
      const after = handle.dataset.after;
      if (handle.hidden || !after) return null;
      return {
        before: id,
        after,
        height: $("section-" + id).getBoundingClientRect().height,
        other: $("section-" + after).getBoundingClientRect().height,
        weight:
          workspaceState.preferences.sectionWeights[id] +
          workspaceState.preferences.sectionWeights[after],
      };
    };
    const resize = (start?: any, delta?: any) => {
      if (!start || start.height + start.other <= 144) return;
      const available = start.height + start.other - 144,
        extra = Math.max(0, Math.min(available, start.height + delta - 72)),
        before = Math.max(
          0.001,
          Math.min(start.weight - 0.001, (start.weight * extra) / available),
        );
      workspaceState.preferences.sectionWeights[start.before] = before;
      workspaceState.preferences.sectionWeights[start.after] =
        start.weight - before;
      updateSectionSizing();
    };
    const finish = () => {
      if (!drag) return;
      drag = null;
      document.body.classList.remove("resizing-sections");
      savePreferences();
    };
    handle.addEventListener("pointerdown", (e?: any) => {
      if (e.button !== 0) return;
      const start = pair();
      if (!start) return;
      drag = { ...start, y: e.clientY, pointer: e.pointerId };
      handle.setPointerCapture(e.pointerId);
      handle.focus({ preventScroll: true });
      document.body.classList.add("resizing-sections");
      e.preventDefault();
    });
    handle.addEventListener("pointermove", (e?: any) => {
      if (drag?.pointer === e.pointerId) resize(drag, e.clientY - drag.y);
    });
    for (const event of ["pointerup", "pointercancel", "lostpointercapture"])
      handle.addEventListener(event, finish);
    handle.addEventListener("keydown", (e?: any) => {
      if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(e.key)) return;
      e.preventDefault();
      resize(
        pair(),
        e.key === "Home"
          ? -10000
          : e.key === "End"
            ? 10000
            : (e.key === "ArrowDown" ? 1 : -1) * (e.shiftKey ? 40 : 10),
      );
      savePreferences();
    });
    handle.addEventListener("dblclick", () => {
      workspaceState.preferences.sectionWeights = {
        rooms: 45,
        private: 20,
        people: 35,
      };
      updateSectionSizing();
      savePreferences();
    });
  }
  new ResizeObserver(() => requestAnimationFrame(updateSectionSizing)).observe(
    $("sidebar-sections"),
  );
  updateSectionSizing();
}
