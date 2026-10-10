import { readPreferences } from "./preferences.js";
// Dynamic records are the bounded research API projections, owned only by this application.
export const workspaceState: Record<string, any> = {};
workspaceState.labels = {
  running: "运行中",
  succeeded: "已完成",
  completed: "已完成",
  active: "运行中",
  revoked: "已撤销",
  blocked: "阻塞",
  ready: "就绪",
  waiting_dependencies: "等待依赖",
  retry_wait: "等待重试",
  paused: "暂停",
  failed: "失败",
  unknown: "未知",
  needs_reconciliation: "待核对",
};
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
workspaceState.personKinds = {
  fixed: "固定成员",
  temporary: "临时成员",
  research: "研究成员",
};
workspaceState.personRoles = {
  research: "研究员",
  review: "质疑者",
  synthesis: "综合者",
  implement: "实现者",
  evaluate: "评估者",
  evaluator: "评估者",
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
