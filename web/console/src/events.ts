import { t } from "./i18n.js";
import { workspaceState } from "./state.js";
import { $ } from "./ui.js";
import { openProfile, maximize } from "./identity.js";
import {
  dismissProfile,
  savePreferences,
  setFollowing,
  initSidebarControls,
  setChatCollapsed,
} from "./preferences.js";
import { render, accept, refresh, connection, connect } from "./observer.js";
import { showPage } from "./navigation.js";
import { updateSectionSizing, initSectionSizing } from "./sizing.js";
import { closeSession, fetchSession } from "./sessions.js";
import { openPerson } from "./people-profile.js";
export function initialize(): void {
  $("open-profile").addEventListener("click", () =>
    openProfile(workspaceState.selected),
  );
  $("close-profile").addEventListener("click", () => {
    dismissProfile(true);
    $("open-profile").focus();
  });
  document.addEventListener("keydown", (e?: any) => {
    if (
      e.key !== "Escape" ||
      $("session-window").open ||
      $("person-window").open
    )
      return;
    if (document.body.classList.contains("profile-open")) dismissProfile(true);
    else if (workspaceState.focusPanel) maximize(workspaceState.focusPanel);
  });
  $("maximize-graph").addEventListener("click", () => maximize("graph"));
  $("maximize-chat").addEventListener("click", () => maximize("chat"));
  $("search").addEventListener("input", render);
  for (const page of ["collaboration", "messages", "settings"]) {
    $("nav-" + page).addEventListener("click", () => {
      location.hash = page;
      showPage(page, true);
    });
  }
  window.addEventListener("hashchange", () => showPage(location.hash.slice(1)));
  for (const category of [
    "people",
    "executors",
    "evolution",
    "observation",
    "service",
  ]) {
    $("settings-nav-" + category).addEventListener("click", () => {
      location.hash = `settings/${category}`;
      showPage(`settings/${category}`);
    });
  }
  for (const id of ["rooms", "private", "people"]) {
    const section = $("section-" + id);
    section.open = !(workspaceState.preferences.collapsed || []).includes(id);
    section.addEventListener("toggle", () => {
      const collapsed = new Set<any>(
        workspaceState.preferences.collapsed || [],
      );
      if (section.open) collapsed.delete(id);
      else collapsed.add(id);
      workspaceState.preferences.collapsed = [...collapsed];
      savePreferences();
      updateSectionSizing();
    });
  }
  $("setting-follow").checked = workspaceState.following;
  $("setting-follow").addEventListener("change", (e?: any) => {
    setFollowing(e.target.checked);
    if (workspaceState.following) workspaceState.chatRoute = "";
    if (workspaceState.data) accept(workspaceState.data);
  });
  $("setting-motion").checked = workspaceState.preferences.motion !== false;
  document.body.classList.toggle(
    "reduced-motion",
    workspaceState.preferences.motion === false,
  );
  $("setting-motion").addEventListener("change", (e?: any) => {
    workspaceState.preferences.motion = e.target.checked;
    document.body.classList.toggle(
      "reduced-motion",
      !workspaceState.preferences.motion,
    );
    savePreferences();
  });
  initSidebarControls();
  initSectionSizing();
  showPage(location.hash.slice(1));
  $("state").addEventListener("change", render);
  $("refresh").addEventListener("click", refresh);
  $("latest-room").addEventListener("click", () => {
    workspaceState.chatRoute = "";
    setFollowing(true);
    if (workspaceState.data) accept(workspaceState.data);
  });
  $("view-map").addEventListener("click", () => {
    document.body.classList.remove("chat-only");
    $("view-map").classList.add("active");
    $("view-chat").classList.remove("active");
  });
  $("view-chat").addEventListener("click", () => {
    setChatCollapsed(false);
    document.body.classList.add("chat-only");
    $("view-chat").classList.add("active");
    $("view-map").classList.remove("active");
  });
  $("chat-title").addEventListener("click", () => {
    workspaceState.chatRoute = "";
    render();
  });
  $("close-session").addEventListener("click", closeSession);
  $("session-window").addEventListener("cancel", (e?: any) => {
    e.preventDefault();
    closeSession();
  });
  $("maximize-session").addEventListener("click", () => {
    const expanded = $("session-window").classList.toggle("expanded");
    $("maximize-session").textContent = expanded
      ? t("events.01831fe8e3")
      : t("events.5a2a22c0d8");
    $("maximize-session").setAttribute("aria-pressed", String(expanded));
    $("maximize-session").setAttribute(
      "aria-label",
      t("events.f65c97353d", {
        p0: expanded ? t("events.ddde089462") : t("events.80f8fbcfa0"),
      }),
    );
  });
  $("session-profile").addEventListener("click", () => {
    const j = workspaceState.historyState?.job;
    const id = workspaceState.sessionAgent;
    closeSession();
    if (j?.profile?.person_id) openPerson(j.profile.person_id);
    else openProfile(id);
  });
  $("session-latest").addEventListener("click", () => {
    if (!workspaceState.historyState || workspaceState.historyState.loading)
      return;
    workspaceState.historyState.older = [];
    workspaceState.historyState.following = true;
    workspaceState.historyState.key = "";
    fetchSession();
    $("session-history").scrollTop = $("session-history").scrollHeight;
  });
  setInterval(connection, 1000);
  refresh();
  connect();
}
