import { t } from "./i18n.js";
import { readPreferences } from "./preferences.js";
// Dynamic records are the bounded research API projections, owned only by this application.
export const workspaceState = {};
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
workspaceState.systemEntries = new Map();
workspaceState.serviceAlerts = new Map();
workspaceState.opened = new Set();
workspaceState.profiles = new Map();
workspaceState.pulses = new Map();
workspaceState.publishPulses = new Map();
workspaceState.lastBytes = new Map();
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
workspaceState.peopleIndex = new Map();
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
