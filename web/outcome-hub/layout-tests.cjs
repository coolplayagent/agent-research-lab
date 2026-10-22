/** Behavior regressions for editable directories and asynchronous research detail ownership. */
const assert = require("node:assert/strict"), fs = require("node:fs"), path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports = {}, globals = {}) {
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  new Function("module", "exports", "require", ...Object.keys(globals), code)(module, module.exports, id => { assert(Object.hasOwn(imports, id), id); return imports[id]; }, ...Object.values(globals));
  return module.exports;
}
class Node extends EventTarget {
  constructor(tag, text = "") {
    super(); this.tag = tag; this.textContent = text; this.children = []; this.value = ""; this.dataset = {}; this.hidden = false; this.disabled = false;
    this.attributes = new Map(); this.style = { setProperty() {} }; this.classes = new Set();
    this.classList = { add: (...names) => names.forEach(name => this.classes.add(name)), remove: (...names) => names.forEach(name => this.classes.delete(name)), contains: name => this.classes.has(name), toggle: (name, enabled = !this.classes.has(name)) => { if (enabled) this.classes.add(name); else this.classes.delete(name); return enabled; } };
  }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
  setAttribute(key, value) { this.attributes.set(key, value); }
  getAttribute(key) { return this.attributes.get(key); }
  scrollIntoView() {}
  reset() {}
  querySelector() { return new Node("button"); }
}
const nodes = new Map(), get = id => { if (!nodes.has(id)) nodes.set(id, new Node("div")); return nodes.get(id); };
const dom = { get, el: (tag, text) => new Node(tag, text), button: (text, onclick) => Object.assign(new Node("button", text), { onclick }), actions: (...controls) => { const row = new Node("div"); row.append(...controls); return row; }, notice: () => {}, open: id => get(id).hidden = false, close: id => get(id).hidden = true, presence: v => v, value: id => get(id).value.trim() };
const i18n = { t: key => key, label: (domain, value) => `${domain}.${value}` };
const base = "web/outcome-hub/src/", contracts = load("web/shared/src/contracts.ts");
const pending = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
const textOf = node => [node.textContent, ...node.children.map(textOf)].join(" ");
(async () => {
  const a = { id: "a", title: "First topic", group_id: "group-a", state: contracts.TopicState.Active, revision: 1 };
  const b = { ...a, id: "b", title: "Second topic", group_id: "group-b" };
  const subject = owner => ({ subject: { id: owner + "-subject", topic_id: owner, name: owner + " subject", kind: contracts.SubjectKind.Repository, reference: "source:main", revision: 1 }, node_count: 0 });
  let catalog = [a, b], heldTopics, heldSubjects, editDone;
  const topics = load(base + "topics.ts", {
    "./deletion.js": { deleteObject: async target => { catalog = catalog.filter(topic => topic.id !== target.id); return true; } },
    "./outcome-board.js": { outcomeBoard: topic => {
      const board = new Node("section", topic.id);
      board.remove = () => { get("topic-detail").children = get("topic-detail").children.filter(node => node !== board); };
      return board;
    } },
    "/assets/shared/contracts.js": contracts, "./dom.js": dom, "./i18n.js": i18n,
    "./icons.js": { icon: () => new Node("svg") }, "./workspace.js": { revealResearch: () => {} }, "./evolution.js": { loadEvolution: async () => {} }, "./conversation.js": { openGroup: async () => {} },
    "./research-forms.js": { topicForm: async (_old, done) => { editDone = done; }, subjectForm: () => {} },
    "./api.js": { query: params => new URLSearchParams(params).toString(), api: async url => {
      if (url.startsWith("/api/v1/collaboration/topics")) return heldTopics ? heldTopics.promise : { topics: catalog.map(topic => ({ topic, goal_count: 0, subject_count: 1 })), next_after: null };
      const id = new URL(url, "http://local").searchParams.get("topic_id");
      return heldSubjects?.id === id ? heldSubjects.promise : { subjects: [subject(id)], next_after: null };
    } },
  }, { location: { hash: "" }, sessionStorage: { setItem() {} }, document: { addEventListener() {}, getElementById: id => get("topic-detail").children.find(node => node.id === id) } });
  topics.initTopics(); await topics.loadTopics();
  const oldCard = get("topic-list").children[0];
  await oldCard.children.at(-1).children[0].onclick();
  await oldCard.children.at(-1).children[1].onclick();
  const renamed = { ...a, title: "Renamed topic", state: contracts.TopicState.Archived, revision: 2 };
  catalog = [renamed, b]; await editDone(renamed);
  assert.equal(get("topic-detail-title").textContent, "Renamed topic", "Use the saved topic identity, never search by its obsolete name");
  assert(get("new-subject").disabled);
  assert(get("subject-list").children[0].children.at(-1).children[1].disabled, "Archived topic edits match backend authority");
  catalog = [a, b]; await topics.loadTopics();
  heldSubjects = { id: "a", ...pending() };
  const first = get("topic-list").children[0].children.at(-1).children[0].onclick();
  await get("topic-list").children[1].children.at(-1).children[0].onclick();
  heldSubjects.resolve({ subjects: [subject("a")], next_after: null }); await first;
  assert.equal(get("topic-detail-title").textContent, "Second topic");
  assert.equal(get("topic-detail").children.find(node => node.id === "topic-outcomes").textContent, "b", "The outcome board must follow the selected topic even when older subject reads finish later");
  assert(textOf(get("subject-list")).includes("b subject"));
  assert(!textOf(get("subject-list")).includes("a subject"), "An earlier topic response must not replace the current topic's subjects");
  heldTopics = pending(); const oldSearch = topics.loadTopics(); const response = heldTopics; heldTopics = null; catalog = [b]; await topics.loadTopics();
  response.resolve({ topics: [{ topic: a, subject_count: 1, goal_count: 0 }], next_after: null }); await oldSearch;
  assert(textOf(get("topic-list")).includes("Second topic")); assert(!textOf(get("topic-list")).includes("First topic"));
  get("close-topic").onclick(); assert(get("topic-detail").hidden);

  const person = { id: "researcher", name: "Researcher", application_id: "research", presence: contracts.Presence.Offline, connections: 0 };
  let writes = [], visible = [person]; const document = new EventTarget(); let refreshed = 0;
  document.addEventListener("workspace-people", () => refreshed++);
  const people = load(base + "people.ts", {
    "/assets/shared/identity.js": { uuid: () => require("node:crypto").randomUUID() },
    "./avatar.js": { avatar: () => new Node("svg") }, "./feedback.js": { busy: () => () => {} }, "./i18n.js": i18n, "./dom.js": dom,
    "./api.js": { query: () => "", api: async () => ({ people: visible, next_after: null }), change: async value => { writes.push(value); visible = []; } },
  }, { document, crypto: require("node:crypto").webcrypto });
  people.initPeople(); await people.loadPeople();
  get("people-list").children[0].children.at(-1).children[0].onclick();
  get("person-name").value = "Renamed researcher";
  await get("person-form").onsubmit({ preventDefault() {} });
  assert.deepEqual(writes[0].person, { id: "researcher", name: "Renamed researcher", application_id: "research" });
  assert.equal(people.display("researcher"), "Renamed researcher", "The identity cache updates even if the active search no longer matches");
  assert.equal(refreshed, 1);

  let picker, memberIds = ["operator", "researcher"];
  class MemberPicker {
    constructor() { picker = this; this.chosen = new Map(); this.excluded = new Set(); }
    reset() { this.chosen.clear(); this.excluded = new Set(["operator"]); }
    async load() {}
  }
  const memberGroup = { id: "group", archived: false, private: false };
  const members = load(base + "members.ts", {
    "/assets/shared/identity.js": { uuid: () => require("node:crypto").randomUUID() },
    "./avatar.js": { avatar: () => new Node("svg") }, "/assets/shared/contracts.js": contracts,
    "./i18n.js": i18n, "./dom.js": dom, "./feedback.js": { busy: () => () => {} },
    "./people.js": { display: id => id }, "./picker.js": { Picker: MemberPicker },
    "./directory.js": { loadGroups: async () => {} },
    "./conversation.js": { current: memberGroup, refreshCurrent: async () => {}, select: async () => {} },
    "./api.js": { query: () => "", api: async () => ({ group: memberGroup, members: { members: memberIds.map(person_id => ({ person_id })), next_after: null } }),
      change: async change => { memberIds = change.add ? [...memberIds, change.person_id] : memberIds.filter(id => id !== change.person_id); } },
  });
  members.initMembers(); get("manage-members").onclick(); await new Promise(setImmediate);
  assert(picker.excluded.has("researcher"));
  await get("member-list").children[1].children.at(-1).onclick();
  assert(!picker.excluded.has("researcher"), "A removed member becomes selectable without reopening the dialog");
  picker.chosen.set("researcher", person); await get("add-members").onclick();
  assert(memberIds.includes("researcher")); assert(picker.excluded.has("researcher"));

  const workspaceDocument = Object.assign(new EventTarget(), { querySelector: () => null, querySelectorAll: () => [] });
  const workspace = load(base + "workspace.ts", { "./dom.js": dom, "./i18n.js": i18n, "./evolution.js": { loadEvolution: async () => {} }, "./iterations.js": { loadIterations: async () => {} } }, { document: workspaceDocument, window: new EventTarget(), matchMedia: () => ({ matches: true }) });
  workspace.initWorkspace();
  get("workspace").classList.add("research-open", "research-expanded");
  get("toggle-directory").onclick();
  assert(get("workspace").classList.contains("directory-open"));
  assert(!get("workspace").classList.contains("research-open"));
  assert(!get("workspace").classList.contains("research-expanded"));
  assert.equal(get("toggle-research").getAttribute("aria-expanded"), "false");
  assert.equal(get("expand-research").getAttribute("aria-pressed"), "false");
  workspace.revealResearch("goals");
  assert(!get("workspace").classList.contains("directory-open"));
  assert(get("workspace").classList.contains("research-open"));
  assert.equal(get("toggle-research").getAttribute("aria-expanded"), "true");
  console.log("Directory ownership, archived controls and identity preservation passed");
})().catch(error => { console.error(error); process.exit(1); });
