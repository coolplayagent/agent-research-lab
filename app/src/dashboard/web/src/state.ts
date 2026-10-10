import { t } from "./i18n.js";
import { readPreferences } from "./preferences.js";
// Dynamic records are the bounded research API projections, owned only by this application.
export const workspaceState: Record<string, any> = {};
workspaceState.preferences = readPreferences();
workspaceState.data = null;
workspaceState.room = "";
workspaceState.selected = "";
workspaceState.following = workspaceState.preferences.follow !== false;
workspaceState.connected = false;
workspaceState.stream = undefined;
workspaceState.fetching = false;
workspaceState.chatKey = "";
workspaceState.chatView = "";
workspaceState.chatRoute = "";
workspaceState.detailKey = "";
workspaceState.inspected = "";
workspaceState.focusPanel = "";
workspaceState.focusScroll = null;
workspaceState.extraSession = null;
workspaceState.sessionAgent = "";
workspaceState.historyState = null;
workspaceState.sessionReturnFocus = null;
workspaceState.snapshotError = "";
workspaceState.systemKey = "";
workspaceState.systemEntries = new Map<any, any>();
workspaceState.serviceAlerts = new Map<any, any>();
workspaceState.opened = new Set<any>();
workspaceState.profiles = new Map<any, any>();
workspaceState.pulses = new Map<any, any>();
workspaceState.publishPulses = new Map<any, any>();
workspaceState.lastBytes = new Map<any, any>();
workspaceState.settingsCategory = "people";
workspaceState.personRoles = {
  research: t("state.fb2f4efd84"),
  review: t("state.f62f3d185a"),
  synthesis: t("state.6153861271"),
  implement: t("state.f8ef43e420"),
  evaluate: t("state.104b51a6e8"),
  evaluator: t("state.104b51a6e8"),
};
workspaceState.peopleRevision = -1;
workspaceState.peopleIndex = new Map<any, any>();
workspaceState.peopleDefaults = {};
workspaceState.peopleModels = {};
workspaceState.executionCatalog = { backends: [], models: [] };
workspaceState.peopleTotal = 0;
workspaceState.peopleCursor = null;
workspaceState.personFocus = null;
workspaceState.personHistory = [];
workspaceState.personHistoryCursor = null;
workspaceState.personRequest = 0;
workspaceState.editingPerson = null;
workspaceState.directoryLoading = false;
workspaceState.peopleKey = "";
workspaceState.memoryNoteRequest = null;
workspaceState.executionEditor = null;
workspaceState.lineageRevision = undefined;
workspaceState.lineageNext = null;
workspaceState.lineageLoading = false;
