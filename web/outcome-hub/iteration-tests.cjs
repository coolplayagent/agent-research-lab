const assert = require("node:assert/strict"), fs = require("node:fs"), path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports = {}, globals = {}) {
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  new Function("module", "exports", "require", ...Object.keys(globals), code)(module, module.exports, id => { assert(Object.hasOwn(imports, id), id); return imports[id]; }, ...Object.values(globals));
  return module.exports;
}
class Node {
  constructor(tag, text = "") { this.tag = tag; this.textContent = text; this.children = []; this.dataset = {}; this.hidden = false; this.disabled = false; this.classList = { contains: () => false }; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children = nodes; }
}
const nodes = new Map(), get = id => { if (!nodes.has(id)) nodes.set(id, new Node("div")); return nodes.get(id); };
const dom = { get, el: (tag, text) => new Node(tag, text), button: (text, onclick) => Object.assign(new Node("button", text), { onclick }), notice: () => {} };
const i18n = { t: (key, parameters) => key + (parameters ? JSON.stringify(parameters) : ""), label: (domain, value) => `${domain}.${value}` };
const textOf = node => [node.textContent, ...node.children.map(textOf)].join(" ");
const base = "web/outcome-hub/src/", contracts = load("web/shared/src/contracts.ts");
const pending = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
const tick = () => new Promise(setImmediate);
(async () => {
  const catalogRequests = [];
  const catalog = load(base + "scenario-catalog.ts", {
    "./api.js": { api: () => { const item = pending(); catalogRequests.push(item); return item.promise; } },
  });
  const firstCatalog = catalog.loadScenarioCatalog(), latestCatalog = catalog.loadScenarioCatalog();
  catalogRequests[1].resolve({ scenarios: [{ id: "custom", public_process: true, manual_review: false }] });
  await latestCatalog;
  catalogRequests[0].resolve({ scenarios: [{ id: "obsolete", public_process: true, manual_review: true }] });
  await firstCatalog;
  assert.equal(catalog.scenarioDescriptor("obsolete"), undefined);
  assert.equal(catalog.scenarioDescriptor("custom").manual_review, false);
  const renderer = load(base + "iteration-render.ts", {
    "/assets/shared/contracts.js": contracts, "./dom.js": dom, "./i18n.js": i18n,
    "./goals.js": { focusGoal: () => {} }, "./people.js": { display: id => id },
  });
  const makeRun = (id, group, revision = 1) => ({
    input: { id, group_id: group, subject_id: "subject", title: `Title ${id}`, objective: "<b>user input</b>", acceptance: "Human criteria", scenario_id: "custom-data-analysis", assignments: Array.from({ length: 6 }, (_, n) => ({ person_id: "person-" + n, instruction: "Task " + n })), max_rounds: 3, total_budget_seconds: 300 },
    state: contracts.IterationState.Running, revision, used_seconds: 0,
    rounds: [{ number: 1, state: contracts.RoundState.Running, goal_id: "goal", goal_attempts: { work: 1 }, baseline_version_id: "baseline", finished_ms: null, candidate: null, evaluation: null, activation: null, feedback: "" }],
  });
  const run = makeRun("one", "a"), rendered = new Node("section");
  rendered.append(...renderer.iterationDetail(run, async () => {}, async () => {}, async () => {}, () => {}, async () => {}));
  assert(textOf(rendered).includes("iterations.unassessed"));
  assert(textOf(rendered).includes("iterations.notActivated"));
  assert(!textOf(rendered).includes("EvaluationState.passed"), "Missing evidence must not imply successful evaluation");
  assert(textOf(rendered).includes("person-5"), "The generic UI has no four-agent limit");
  assert(textOf(rendered).includes("<b>user input</b>"), "User text remains text content");
  const accepted = structuredClone(run);
  accepted.rounds[0].candidate = { version_id: "candidate", producer_work_id: "author", state: contracts.CandidateState.Accepted };
  accepted.rounds[0].evaluation = { evaluator_work_id: "reviewer", goal_revision: 8, state: contracts.EvaluationState.Passed, summary: "Assessed", evidence: { checks: "recorded" } };
  const evaluated = new Node("section");
  evaluated.append(...renderer.iterationDetail(accepted, async () => {}, async () => {}, async () => {}, () => {}, async () => {}));
  assert(textOf(evaluated).includes("EvaluationState.passed"));
  assert(textOf(evaluated).includes("iterations.notActivated"), "Host acceptance never masquerades as deployment");
  const policyOwned = new Node("section");
  policyOwned.append(...renderer.iterationDetail(accepted, async () => {}, async () => {}, async () => {}, () => {}));
  assert(!textOf(policyOwned).includes("iterations.review"), "Policy-owned scenario review is not offered as a manual shortcut");
  const switching = structuredClone(accepted);
  switching.rounds[0].activation = { state: contracts.ActivationState.Pending, reference: "switch-operation", evidence: { claim_id: "claim" } };
  const switchingView = new Node("section");
  switchingView.append(...renderer.iterationDetail(switching, async () => {}, async () => {}, async () => {}, () => {}, async () => {}));
  const descendants = node => [node, ...node.children.flatMap(descendants)];
  const switchingControls = descendants(switchingView).filter(node => ["iterations.pause", "iterations.stop"].includes(node.textContent));
  assert.equal(switchingControls.length, 2);
  assert(switchingControls.every(node => node.disabled), "In-flight activation cannot offer cancellation before host settlement");
  assert(textOf(switchingView).includes("iterations.activationPending"));
  assert(!textOf(switchingView).includes("ActivationState.active"));
  assert(!textOf(switchingView).includes("iterations.review"));

  get("goal-form").reset = () => {};
  const goalForm = load(base + "goal-form.ts", {
    "/assets/shared/identity.js": { uuid: () => require("node:crypto").randomUUID() },
    "/assets/shared/contracts.js": contracts, "./i18n.js": i18n,
    "./feedback.js": { busy: async (_node, action) => action() },
    "./conversation.js": { current: { id: "a", kind: contracts.GroupKind.Conversation, archived: false } },
    "./goals.js": { loadGoals: async () => {} }, "./people.js": { display: id => id },
    "./dom.js": { ...dom, open: () => {}, close: () => {}, value: id => get(id).value },
    "./api.js": { query: args => new URLSearchParams(args).toString(), api: async url => url === "/api/v1/collaboration/scenarios"
      ? { scenarios: [{ id: "collaboration", fields: [] }, { id: "custom-role-pipeline", fields: [], assignment_roles: ["author", "checker"] }] }
      : { members: { members: [], next_after: null } } },
  }, { crypto: require("node:crypto").webcrypto });
  await goalForm.openGoal();
  assert.deepEqual(get("goal-scenario").children.map(node => node.value), ["collaboration"], "Role pipelines use the iteration form that can configure their roles");

  get("iterations-page").hidden = true;
  let held, visible = [], controls, writes = [], failWrite = false, latest = [];
  const iterations = load(base + "iterations.ts", {
    "/assets/shared/identity.js": { uuid: () => require("node:crypto").randomUUID() },
    "/assets/shared/contracts.js": contracts, "./dom.js": dom, "./i18n.js": i18n,
    "./iteration-form.js": { iterationForm: async () => {} },
    "./iteration-review.js": { reviewIteration: async () => {} },
    "./scenario-catalog.js": { scenarioDescriptor: () => ({ manual_review: true }) },
    "./iteration-render.js": {
      iterationCard: (run, open) => { visible.push(run); return dom.button(run.input.id, open); },
      iterationDetail: (_run, pause, resume, stop) => { controls = { pause, resume, stop }; return []; },
    },
    "./workspace.js": { revealResearch: () => {} },
    "./evolution.js": { refreshEvolution: () => {}, focusEvolutionSubject: async () => true },
    "./api.js": { query: args => new URLSearchParams(args).toString(), api: async (url, body) => {
      if (body) { writes.push(body); if (failWrite) throw new Error("response lost"); return {}; }
      return held ? held.promise : { runs: latest, next_after: null };
    } },
  }, { crypto: require("node:crypto").webcrypto, confirm: () => true });
  iterations.initIterations();
  iterations.selectIterations({ id: "a", kind: contracts.GroupKind.Conversation, archived: false });
  held = pending(); const earlier = iterations.loadIterations(); const reply = held;
  iterations.selectIterations({ id: "b", kind: contracts.GroupKind.Conversation, archived: false });
  held = null; latest = [makeRun("two", "b", 4)]; await iterations.loadIterations();
  reply.resolve({ runs: [run], next_after: null }); await earlier;
  assert.equal(get("iteration-list").children[0].textContent, "two", "Old group responses are fenced");
  get("iteration-list").children[0].onclick();
  failWrite = true;
  await assert.rejects(controls.pause(), /response lost/);
  await assert.rejects(controls.pause(), /response lost/);
  assert.equal(writes[0].request_id, writes[1].request_id, "A retry at the same revision retains its operation identity");
  assert.equal(writes[0].revision, 4);
  assert.deepEqual(writes[0].action, { operation: "pause" });
  failWrite = false; visible = [];
  latest = [makeRun("two", "b", 2)]; await iterations.loadIterations();
  assert.equal(visible[0].revision, 4, "A lagging response must not regress visible evidence");
  latest = [run]; await iterations.loadIterations();
  assert.equal(get("iteration-status").textContent, "Error: iterations.scopeMismatch");
  assert.equal(get("iteration-list").children[0].textContent, "two");
  console.log("Verified arbitrary scenarios/participants, separate assessment and activation, request fences, revision monotonicity and idempotent controls.");
})().catch(error => { console.error(error); process.exit(1); });
