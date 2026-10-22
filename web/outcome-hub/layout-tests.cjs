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
  const workspace = load(base + "workspace.ts", { "./dom.js": dom, "./i18n.js": i18n }, { document: workspaceDocument, window: new EventTarget(), matchMedia: () => ({ matches: true }) });
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
