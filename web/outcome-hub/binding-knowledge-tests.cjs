/** Regressions for first-use agent bindings and independent knowledge settings. */
const assert = require("node:assert/strict"), fs = require("node:fs"), path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE), base = "web/outcome-hub/src/";
function load(file, imports = {}, globals = {}) {
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  new Function("module", "exports", "require", ...Object.keys(globals), code)(module, module.exports, id => { assert(Object.hasOwn(imports, id), id); return imports[id]; }, ...Object.values(globals));
  return module.exports;
}
class Node extends EventTarget {
  constructor(tag, text = "") { super(); this.tag = tag; this.textContent = text; this.children = []; this.attributes = new Map(); this.hidden = false; this.disabled = false; this.connected = false; this._value = ""; this.dataset = {}; this.classList = { toggle() {}, remove() {} }; }
  get isConnected() { return this.connected || !!this.parent?.isConnected; }
  get childElementCount() { return this.children.length; }
  get options() { return this.children; }
  get value() { return this._value; }
  set value(value) { this._value = this.tag === "select" && !this.children.some(o => o.value === value) ? "" : value; }
  append(...nodes) { for (const node of nodes) { node.parent = this; this.children.push(node); } }
  replaceChildren(...nodes) { for (const child of this.children) child.parent = null; this.children = []; this.append(...nodes); }
  setAttribute(key, value) { this.attributes.set(key, value); }
  getAttribute(key) { return this.attributes.get(key); }
  reset() {}
  querySelector() { return new Node("button"); }
  scrollIntoView() {}
  focus() { this.focused = true; }
}
const nodes = new Map(), get = id => { if (!nodes.has(id)) { const n = new Node("div"); n.connected = true; nodes.set(id, n); } return nodes.get(id); };
const el = (tag, text) => new Node(tag, text), textOf = n => [n.textContent, ...n.children.map(textOf)].join(" ");
const find = (node, predicate) => [node, ...node.children.flatMap(n => find(n, predicate))].filter(predicate);
const buttonIn = (node, text) => find(node, n => n.tag === "button" && n.textContent === text)[0];
const bindingForm = panel => find(panel, n => n.tag === "form" && n.className === "settings-form binding-form")[0];
const i18n = { t: (key, params = {}) => key + (Object.keys(params).length ? JSON.stringify(params) : ""), label: (domain, value) => `${domain}.${value}` };
let box;
const forms = {
  element: el, input: (value, type) => Object.assign(el("input"), { value, type }), check: checked => Object.assign(el("input"), { checked }), area: value => Object.assign(el("textarea"), { value }),
  select: (options, value) => { const node = el("select"); node.append(...options.map(([id, title]) => Object.assign(el("option", title), { value: id }))); node.value = value; return node; },
  field: (title, control) => { const label = el("label", title); label.append(control); return label; },
  dialog: () => box = { form: el("form"), dialog: el("dialog"), status: el("p") },
  finish: (dialog, save) => { dialog.save = save; },
};
const field = (node, label) => find(node, n => n.tag === "label" && n.textContent === label)[0].children[0];
const dom = { el, get, actions: (...controls) => { const n = el("div"); n.append(...controls); return n; }, button: (text, onclick) => Object.assign(el("button", text), { onclick }), notice: () => {}, open: id => get(id).hidden = false, close: id => get(id).hidden = true, presence: v => v, value: id => get(id).value.trim() };
const feedback = { busy: node => { if (node.pending) return null; node.pending = true; return () => node.pending = false; } };
const contracts = load("web/shared/src/contracts.ts"), { OutcomeKind, OutcomeServiceKind, Presence } = contracts;
const tick = () => new Promise(setImmediate), pending = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
const person = (id, name = id) => ({ id, name, application_id: null, presence: Presence.Offline, connections: 0 });
const config = () => ({ revision: 12, services: [], adapters: [{ id: "first" }, { id: "second" }], bindings: {}, outcome_services: [] });
(async () => {
  let bindingWrites = [], bindingRecords = [], visibleProgress = [], requestSequence = 0;
  const timers = [];
  let catalog = [person("operator")], failRead = false, failCreate = false, held, saved, changes = [];
  const api = async (url, body) => {
    if (url === "/api/v1/collaboration/bindings") {
      if (!body) return bindingRecords;
      bindingWrites.push(body);
      const record = { request: body, state: contracts.BindingOperationState.Running };
      bindingRecords = bindingRecords.filter(r => r.request.person_id !== body.person_id).concat(record);
      return record;
    }
    if (failRead) throw new Error("directory unavailable");
    if (held) return held.promise;
    const after = new URL(url, "http://local").searchParams.get("after");
    const list = catalog.filter(p => p.id > (after || "")).sort((a, b) => a.id.localeCompare(b.id));
    return { people: list.slice(0, 1), next_after: list.length > 1 ? list[0].id : null };
  };
  const document = new EventTarget();
  const people = load(base + "people.ts", {
    "/assets/shared/identity.js": { uuid: () => "new" }, "./avatar.js": { avatar: () => el("svg") }, "./feedback.js": feedback,
    "./i18n.js": i18n, "./dom.js": dom, "./api.js": { api, query: params => new URLSearchParams(params).toString(), change: async value => {
      if (failCreate) throw new Error("create failed"); changes.push(value); catalog.push(person(value.person.id, value.person.name));
    } },
  }, { document });
  people.initPeople();
  const bindings = load(base + "binding-settings.ts", {
    "/assets/shared/contracts.js": contracts, "/assets/shared/identity.js": { uuid: () => "request-" + ++requestSequence },
    "./binding-progress.js": { bindingEndpoint: "/api/v1/collaboration/bindings", showProgress(_container, records, id) { visibleProgress = records.filter(record => record.request.request_id === id); } },
    "./api.js": { api, ApiError: class extends Error {} }, "./dom.js": dom, "./forms.js": forms, "./feedback.js": feedback, "./i18n.js": i18n, "./people.js": people,
    "./settings-guard.js": { trackSettings() {}, savedSettings() {} },
  }, { document, window: { setTimeout(callback) { timers.push(callback); } } });
  const render = (cfg = config()) => bindings.renderBindings(cfg, [], async next => saved = next, async () => {});
  bindingRecords = [{ request: { request_id: "historical", person_id: "old-person" }, state: contracts.BindingOperationState.Succeeded }];
  render(); await tick();
  assert.deepEqual(visibleProgress, [], "Historical validation logs are hidden on entry");
  let panel = get("binding-list"), form = bindingForm(panel), picker = field(form, "services.person"), adapter = field(form, "services.agent");
  assert(panel.children.indexOf(form) < panel.children.findIndex(n => n.className === "binding-progress-list"), "The binding form precedes validation logs");
  assert.equal(picker.children.length, 0, "The operator is never a bindable digital person");
  assert(buttonIn(form, "binding.validate").disabled); assert(textOf(form).includes("bindings.noPeople"));
  adapter.value = "second";
  field(form, "binding.program").value = "/trusted/program"; field(form, "binding.model").value = "chosen-model";
  await buttonIn(form, "bindings.createPerson").onclick();
  assert.equal(get("person-dialog").hidden, false);
  get("person-name").value = "Ada"; failCreate = true;
  await get("person-form").onsubmit({ preventDefault() {} });
  assert.equal(changes.length, 0); assert.equal(adapter.value, "second"); assert.equal(picker.value, "");
  failCreate = false;
  await get("person-form").onsubmit({ preventDefault() {} }); await tick();
  assert.equal(picker.value, "agent-new"); assert.equal(adapter.value, "second"); assert(!buttonIn(form, "binding.validate").disabled);
  assert.equal(changes[0].operation, "person");
  await form.onsubmit({ preventDefault() {} });
  assert.equal(saved, undefined, "Creating a person never bypasses binding validation");
  assert.equal(bindingWrites[0].revision, 12); assert.equal(bindingWrites[0].person_id, "agent-new");
  assert.equal(bindingWrites[0].adapter_id, "second");
  assert.equal(visibleProgress.length, 1);
  assert.equal(visibleProgress[0].request.request_id, bindingWrites[0].request_id, "The form selects only its new validation log");
  assert.equal(bindingWrites[0].program, "/trusted/program"); assert.equal(bindingWrites[0].model, "chosen-model");
  await buttonIn(form, "bindings.createPerson").onclick(); dom.close("person-dialog");
  assert.equal(changes.length, 1, "Canceling creation performs no write"); assert.equal(adapter.value, "second");

  failRead = true; render(); await tick();
  form = bindingForm(panel);
  assert(textOf(form).includes("bindings.loadFailed")); assert(!textOf(form).includes("bindings.noPeople"));
  failRead = false; await buttonIn(form, "bindings.refreshPeople").onclick();
  assert.equal(field(form, "services.person").value, "agent-new"); assert(!textOf(form).includes("bindings.loadFailed"));
  catalog = [person("a"), person("b"), person("operator")];
  await buttonIn(form, "bindings.refreshPeople").onclick();
  picker = field(form, "services.person"); assert.deepEqual(picker.children.map(n => n.value), ["a", "b"]);
  held = pending(); const old = held; const refresh = buttonIn(form, "bindings.refreshPeople").onclick();
  held = null; render(); await tick();
  old.resolve({ people: [person("obsolete")], next_after: null }); await refresh;
  assert.equal(field(bindingForm(panel), "services.person").value, "a", "Detached responses cannot replace the new form");
  held = pending(); const initial = held; render();
  held = null; await buttonIn(bindingForm(panel), "bindings.refreshPeople").onclick();
  initial.resolve({ people: [person("late")], next_after: null }); await tick();
  assert.equal(field(bindingForm(panel), "services.person").value, "a", "Older refreshes cannot overwrite the latest directory");

  const configured = config(); configured.bindings = { b: "second" };
  render(configured); await tick();
  form = bindingForm(panel);
  assert(panel.children.indexOf(form) < panel.children.findIndex(n => n.className === "binding-progress-list"), "Existing bindings keep the form above validation logs");
  const writesBeforeReverify = bindingWrites.length;
  await buttonIn(panel, "binding.reverify").onclick();
  assert.equal(bindingWrites.length, writesBeforeReverify + 1, "Reverify starts a fresh validation request");
  assert.equal(bindingWrites.at(-1).person_id, "b");
  assert.equal(bindingWrites.at(-1).adapter_id, "second");
  assert.equal(bindingWrites.at(-1).program, null);
  assert.equal(bindingWrites.at(-1).model, null);
  assert(buttonIn(panel, "binding.reverify").disabled, "Running validation cannot be started twice");
  assert.equal(visibleProgress.length, 1);
  const selected = bindingWrites.at(-1).request_id;
  assert.equal(visibleProgress[0].request.request_id, selected, "Reverify replaces the previous person's displayed log");
  bindingRecords = bindingRecords.map(record => ({ ...record, state: contracts.BindingOperationState.Succeeded }));
  timers.pop()(); await tick();
  assert.equal(visibleProgress.length, 1);
  assert.equal(visibleProgress[0].request.request_id, selected, "Earlier completed operations cannot take over the selected log");
  render(configured); await tick();
  assert.equal(visibleProgress[0].request.request_id, selected, "A settings refresh retains the selected completed log");
  bindings.resetBindingProgress(); render(configured); await tick();
  assert.deepEqual(visibleProgress, [], "Re-entering the page does not restore historical logs");

  const noAdapters = config(); noAdapters.adapters = []; render(noAdapters); await tick();
  assert(buttonIn(bindingForm(panel), "binding.validate").disabled);
  assert(textOf(bindingForm(panel)).includes("bindings.noAdapters"));
  assert.equal(find(panel, n => n.tag === "a")[0].href, "#settings/adapters");

  const kind = Object.values(OutcomeKind)[0], cfg = config();
  cfg.outcome_services = [
    { id: "knowledge", kind: OutcomeServiceKind.Knowledge, management: contracts.ServiceManagement.LocalManaged, enabled: true, endpoint: "unix:///knowledge", max_seconds: 120, dependencies: [], outcome_kinds: [kind] },
    { id: "disabled-knowledge", kind: OutcomeServiceKind.Knowledge, enabled: false, endpoint: "unix:///disabled", max_seconds: 120, dependencies: [], outcome_kinds: [kind] },
  ];
  cfg.knowledge_groups = { group: { service_id: "knowledge", repository: "repo" } };
  const view = { configuration: cfg, outcome_health: [] }, save = async next => saved = next;
  const targetForms = load(base + "knowledge-form.ts", {
    "/assets/shared/contracts.js": contracts, "./i18n.js": i18n, "./dom.js": dom, "./forms.js": forms,
    "./settings-guard.js": { trackSettings() {} },
  });
  const bindingForms = load(base + "knowledge-bindings.ts", {
    "/assets/shared/contracts.js": contracts, "./i18n.js": i18n, "./dom.js": dom, "./forms.js": forms,
    "./settings-guard.js": { trackSettings() {} },
  });
  const cards = load(base + "knowledge-card.ts", { "/assets/shared/contracts.js": contracts, "./i18n.js": i18n, "./dom.js": dom });
  const requests = [], directory = { kind: contracts.KnowledgeBackend.Directory, access: contracts.KnowledgeAccess.ReadOnly, enabled: true, path: "/external", content_root: ".", reference: null, remote: null, credential_env: [] };
  let targetConfig = { revision: 8, targets: { repo: { ...directory }, "new-repo": { ...directory }, stopped: { ...directory, enabled: false } } };
  const knowledge = load(base + "knowledge-settings.ts", {
    "/assets/shared/contracts.js": contracts, "./i18n.js": i18n, "./dom.js": dom,
    "./knowledge-form.js": targetForms, "./knowledge-card.js": cards, "./knowledge-bindings.js": bindingForms,
    "./api.js": { api: async (_url, body, intent) => {
      requests.push({ body, intent });
      if (body.request.operation === "probe") return { revision: "abc123" };
      if (body.request.operation === "save") targetConfig = structuredClone(body.request.configuration);
      return structuredClone(targetConfig);
    } },
  }, { confirm: () => true });
  knowledge.renderKnowledgeSettings(view, save); await tick();
  const targetsPanel = get("knowledge-targets");
  assert(buttonIn(targetsPanel, "knowledge.add"));
  assert(!buttonIn(targetsPanel, "knowledge.addGit") && !buttonIn(targetsPanel, "knowledge.addDirectory"), "Both types share one add entry");
  assert(textOf(targetsPanel).includes("/external"), "Stored details load automatically");
  assert(!textOf(targetsPanel).includes("unix:///knowledge"));
  assert(!textOf(targetsPanel).includes("disabled-knowledge"));
  await buttonIn(targetsPanel, "knowledge.binding").onclick(); await tick();
  field(box.form, "knowledge.scope").value = "group"; field(box.form, "knowledge.scopeId").value = "group";
  await field(box.form, "knowledge.scope").onchange();
  const targetPicker = field(box.form, "knowledge.selectTarget");
  assert.equal(targetPicker.tag, "select"); assert.equal(targetPicker.value, "repo");
  assert.deepEqual(targetPicker.children.map(n => n.value), ["repo", "new-repo"], "Only added, enabled knowledge bases can be bound");
  targetPicker.value = "new-repo"; await box.save();
  assert.equal(saved.knowledge_groups.group.repository, "new-repo");
  await buttonIn(targetsPanel.children.at(-1), "services.unbind").onclick();
  assert.equal(saved.knowledge_groups.group, undefined);
  const targetServiceCard = targetsPanel.children[1].children[0];
  assert.equal(requests[0].intent, "manage-knowledge"); assert.equal(requests[0].body.configuration_revision, 12);
  const repoCard = find(targetServiceCard, n => n.tag === "article" && textOf(n).includes("repo"))[0];
  await buttonIn(repoCard, "services.remove").onclick();
  assert(textOf(repoCard).includes("knowledge.removeBound"));
  assert(!requests.some(r => r.body.request.operation === "save"), "Deleting a bound target cannot leave dangling defaults");
  await buttonIn(repoCard, "knowledge.probe").onclick(); assert(textOf(repoCard).includes("abc123"));
  await buttonIn(repoCard, "knowledge.disable").onclick();
  assert.equal(targetConfig.targets.repo.enabled, false);
  await buttonIn(targetsPanel, "knowledge.binding").onclick(); await tick();
  field(box.form, "knowledge.scopeId").value = "group-one";
  field(box.form, "knowledge.selectTarget").value = "new-repo";
  await field(box.form, "knowledge.scopeId").onchange();
  assert.equal(field(box.form, "knowledge.selectTarget").value, "new-repo", "Entering a new scope ID preserves the chosen store");
  await box.save();
  assert.deepEqual(saved.knowledge_groups["group-one"], { service_id: "knowledge", repository: "new-repo" });
  await buttonIn(targetsPanel, "knowledge.binding").onclick(); await tick();
  field(box.form, "knowledge.scope").value = "goal"; field(box.form, "knowledge.scopeId").value = "goal-one";
  await field(box.form, "knowledge.scope").onchange();
  await box.save(); assert.deepEqual(saved.knowledge_goals["goal-one"], { service_id: "knowledge", repository: "new-repo" });

  // A missing existing default requires explicit replacement and may not silently switch.
  const missingCfg = structuredClone(cfg); missingCfg.knowledge_groups = { group: { service_id: "knowledge", repository: "missing" } };
  bindingForms.bindScope(missingCfg, save, async () => targetConfig, "group", "group"); await tick();
  assert.equal(field(box.form, "knowledge.selectTarget").value, "");
  await assert.rejects(box.save(), /knowledge.noAvailableTargets/);
  field(box.form, "knowledge.selectTarget").value = "new-repo"; await box.save();
  assert.equal(saved.knowledge_groups.group.repository, "new-repo");
  const secondCfg = structuredClone(cfg); secondCfg.outcome_services.push({ ...cfg.outcome_services[0], id: "second-knowledge" });
  const late = pending();
  bindingForms.bindScope(secondCfg, save, service => service === "knowledge" ? late.promise : Promise.resolve({ revision: 1, targets: { second: directory } }));
  field(box.form, "knowledge.service").value = "second-knowledge";
  await field(box.form, "knowledge.service").onchange();
  late.resolve(targetConfig); await tick();
  assert.equal(field(box.form, "knowledge.selectTarget").value, "second", "Late responses from another service cannot replace the current options");
  bindingForms.bindScope(cfg, save, async () => { throw new Error("offline"); }); await tick();
  assert(field(box.form, "knowledge.selectTarget").disabled); await assert.rejects(box.save(), /knowledge.noAvailableTargets/);
  await buttonIn(box.form, "knowledge.load").onclick(); assert(field(box.form, "knowledge.selectTarget").disabled);

  let addedTarget, inspected;
  targetForms.knowledgeForm(contracts.KnowledgeBackend.Directory, async (name, target) => { addedTarget = { name, target }; }, async target => { inspected = target; return { target, revision: "directory-revision" }; });
  field(box.form, "knowledge.id").value = "team-notes"; field(box.form, "knowledge.path").value = "/external/notes";
  assert.equal(find(box.form, node => node.tag === "select").length, 0, "Directory creation requires no type or service dropdown");
  await buttonIn(box.form, "knowledge.checkPath").onclick();
  assert.equal(inspected.kind, contracts.KnowledgeBackend.Directory); assert.equal(inspected.content_root, ".");
  await box.save(); assert.equal(addedTarget.target.access, contracts.KnowledgeAccess.ReadOnly); assert.equal(addedTarget.target.reference, null);
  targetForms.knowledgeForm(contracts.KnowledgeBackend.Git, async (name, target) => { addedTarget = { name, target }; }, async target => ({ target: { ...target, reference: "refs/heads/main" }, revision: "git-revision" }));
  await buttonIn(box.form, "knowledge.checkPath").onclick();
  assert.equal(field(box.form, "knowledge.branch").value, "refs/heads/main");
  const writable = field(box.form, "knowledge.writable"); writable.checked = true; writable.onchange();
  assert(field(box.form, "knowledge.branch").required, "Writing requires an explicitly prepared branch");
  assert(find(box.form, node => node.tag === "details")[0].open);
  const directoryType = find(box.form, node => node.type === "radio" && node.value === contracts.KnowledgeBackend.Directory)[0];
  directoryType.onchange(); await box.save();
  assert.equal(addedTarget.target.kind, contracts.KnowledgeBackend.Directory);
  assert.equal(addedTarget.target.reference, null); assert.equal(field(box.form, "knowledge.branch").required, false);
  assert(field(box.form, "knowledge.branch").disabled, "Hidden Git fields cannot block directory submission");

  let servicesLoaded = 0, progressResets = 0;
  const settingPanels = ["services", "knowledge", "bindings"].map(name => Object.assign(el("section"), { dataset: { settingsPanel: name } }));
  const settingLinks = ["services", "knowledge", "bindings"].map(name => Object.assign(el("a"), { dataset: { setting: name } }));
  const location = { hash: "#settings/knowledge" };
  const navigation = load(base + "navigation.ts", {
    "./api.js": { api() {}, query() {} }, "./conversation.js": { select() {} },
    "./tasks.js": { loadTasks() {}, loadCoordinator() {} }, "./outcome-detail.js": { showOutcome() {} }, "./dom.js": dom, "./i18n.js": i18n,
    "./binding-settings.js": { resetBindingProgress: () => progressResets++ },
    "./settings.js": { loadServices: () => servicesLoaded++ }, "./icons.js": { icon() {} }, "./people.js": { loadPeople() {} },
    "./applications.js": { loadApplications() {} }, "./settings-guard.js": { allowSettingsLeave: () => true, initSettingsGuard() {} },
    "./workspace.js": { revealResearch() {}, syncControls() {} },
  }, { location, history: { replaceState(_state, _title, hash) { location.hash = hash; } }, document: {
    getElementById: () => null,
    querySelectorAll: selector => selector === "[data-settings-panel]" ? settingPanels : selector === "[data-setting]" ? settingLinks : [],
  } });
  navigation.navigate();
  assert.equal(get("page-title").textContent, "knowledge.title");
  assert.deepEqual(settingPanels.map(n => n.hidden), [true, false, true]);
  assert.equal(settingLinks[1].getAttribute("aria-current"), "page"); assert.equal(servicesLoaded, 1);
  location.hash = "#settings/services"; navigation.navigate();
  assert.deepEqual(settingPanels.map(n => n.hidden), [false, true, true]);
  const previousResets = progressResets;
  location.hash = "#settings/bindings"; navigation.navigate();
  assert.equal(progressResets, previousResets + 1);
  navigation.navigate(); assert.equal(progressResets, previousResets + 1, "Refreshing the current route preserves the selected log");
  location.hash = "#settings/knowledge"; navigation.navigate();
  assert.equal(progressResets, previousResets + 2, "Leaving digital person management clears the log selection");

  for (const hash of ["#evolution", "#iterations", "#settings/status"]) {
    location.hash = hash; navigation.navigate();
    assert.equal(location.hash, hash.startsWith("#settings/") ? "#settings/bindings" : "#groups");
    assert.equal(get("page-title").textContent, hash.startsWith("#settings/") ? "workspace.bindings" : "navigation.f3f8bcf3f5");
  }

  let bindingRendered = false;
  const settings = load(base + "settings.ts", {
    "/assets/shared/contracts.js": contracts, "./i18n.js": i18n, "./dom.js": dom, "./forms.js": forms, "./feedback.js": feedback,
    "./api.js": { api: async url => { assert.equal(url, "/api/v1/collaboration/services", "Directory reads must not block service settings"); return { ...view, health: [], restart_required: false }; } },
    "./settings-guard.js": { trackSettings() {}, savedSettings() {}, allowSettingsLeave: () => true, hasUnsavedSettings: () => false },
    "./adapter-settings.js": { renderAdapters() {} },
    "./knowledge-settings.js": { renderKnowledgeSettings() {} }, "./service-forms.js": {}, "./adapter-forms.js": {}, "./execution-status.js": { renderExecutionStatus() {} },
    "./binding-settings.js": { renderBindings() { bindingRendered = true; } },
  });
  await settings.loadServices(); assert(bindingRendered, "Service settings render before independent directory requests");

  const progressView = load(base + "binding-progress.ts", {
    "/assets/shared/contracts.js": contracts, "./api.js": { api }, "./forms.js": forms, "./i18n.js": i18n,
  });
  const logPanel = el("div");
  const attempt = (person, id, started, adapter = "codex") => ({ request: { person_id: person, request_id: id, adapter_id: adapter }, started_unix_ms: started, state: contracts.BindingOperationState.Failed, steps: [] });
  const attempts = [attempt("same-person", "old", 1, "cc"), attempt("same-person", "latest", 2)];
  progressView.showProgress(logPanel, attempts);
  assert.equal(logPanel.children.length, 0, "No log is shown without an explicit selection");
  progressView.showProgress(logPanel, attempts, "latest");
  assert.equal(logPanel.children.length, 1);
  assert(textOf(logPanel).includes("binding.logTitle"));
  assert(!textOf(logPanel).includes('"adapter":"cc"'));
  assert(textOf(logPanel).includes('"adapter":"codex"'));
  assert.equal(attempts.length, 2, "Rendering never mutates the poll response");
  const many = [...attempts, ...Array.from({ length: 12 }, (_, index) => attempt(`person-${index}`, `id-${index}`, index + 3))];
  progressView.showProgress(logPanel, many, "id-8");
  assert.equal(logPanel.children.length, 1, "The page has exactly one log across all people");
  assert(textOf(logPanel).includes('"person":"person-8"'));
  progressView.showProgress(logPanel, many, "not-returned-yet"); assert.equal(logPanel.children.length, 0, "A newly clicked validation immediately clears the previous log");
  progressView.showProgress(logPanel, many); assert.equal(logPanel.children.length, 0);


  const html = fs.readFileSync(path.join(root, "web/outcome-hub/index.html"), "utf8");
  assert.match(html, /href="#settings\/knowledge" data-setting="knowledge"/);
  assert.match(html, /data-settings-panel="knowledge" hidden>\s*<div id="knowledge-targets"><\/div>/);
  assert(!html.includes("knowledge-services"));
  assert(!html.includes("knowledge.advancedConnections"));
  console.log("First-use binding, directory recovery/races, knowledge service ownership and independent topic configuration passed");
})().catch(error => { console.error(error); process.exit(1); });
