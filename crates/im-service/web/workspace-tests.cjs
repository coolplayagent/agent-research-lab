/** Behavioral regressions for context fences, drafts and evidence-only graph edges. */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function moduleFrom(file, imports = {}, globals = {}) {
  const source = fs.readFileSync(path.join(root, file), "utf8");
  const code = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
    },
  }).outputText;
  const module = { exports: {} };
  new Function("module", "exports", "require", ...Object.keys(globals), code)(
    module,
    module.exports,
    (id) => {
      assert(Object.hasOwn(imports, id), `Unexpected import: ${id}`);
      return imports[id];
    },
    ...Object.values(globals),
  );
  return module.exports;
}
const base = "crates/im-service/web/src/";
const contracts = moduleFrom("web/shared/src/contracts.ts");
class Node extends EventTarget {
  constructor(tag = "div", text = "") {
    super();
    this.tag = tag;
    this.textContent = text;
    this.children = [];
    this.dataset = {};
    this.style = {};
    this.attributes = new Map();
    this.value = "";
    this.hidden = false;
    this.isConnected = true;
    this.scrollTop = 0;
    this.scrollHeight = 100;
    this.clientHeight = 100;
  }
  append(...nodes) {
    this.children.push(...nodes);
  }
  replaceChildren(...nodes) {
    this.children = nodes;
  }
  get childElementCount() {
    return this.children.length;
  }
  querySelectorAll(selector) {
    const key = selector.includes("data-goal-id") ? "goalId" : selector.includes("data-evidence-key") ? "evidenceKey" : "sequence";
    return this.children.flatMap((node) => [node, ...node.querySelectorAll(selector)])
      .filter((node) => node.dataset[key] && (!selector.includes("[open]") || node.open));
  }
  setAttribute(key, value) { this.attributes.set(key, value); }
  getAttribute(key) { return this.attributes.get(key) ?? null; }
  removeAttribute(key) { this.attributes.delete(key); }
  focus() {}
  scrollIntoView() {}
}
const elements = new Map();
const get = (id) => {
  if (!elements.has(id)) elements.set(id, new Node());
  return elements.get(id);
};
const dom = {
  get,
  el: (tag, text) => new Node(tag, text),
  button: (text, action) =>
    Object.assign(new Node("button", text), { onclick: action }),
  notice: () => {},
};
const i18n = { t: (key) => key };
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((r, j) => {
    resolve = r;
    reject = j;
  });
  return { promise, resolve, reject };
};
(async () => {
  const { resultForMessage } = moduleFrom(base + "message-results.ts", { "/assets/shared/contracts.js": contracts });
  const submitted = { group_id: "team", event: { kind: contracts.GoalEventKind.Submitted, goal_id: "goal", work_id: "work", person_id: "member", attempt: 2 } };
  const goalResult = { input: { id: "goal", group_id: "team" }, work: [{ id: "work", person_id: "member", attempt: 2, state: contracts.WorkState.Submitted, result: { succeeded: true, summary: "Recorded result" } }] };
  assert.equal(resultForMessage(submitted, [goalResult]).result.summary, "Recorded result");
  for (const patch of [{attempt:1}, {person_id:"other"}, {goal_id:"other"}, {kind:contracts.GoalEventKind.Claimed}]) {
    assert.equal(resultForMessage({...submitted, event:{...submitted.event, ...patch}}, [goalResult]), null);
  }
  assert.equal(resultForMessage({...submitted, group_id:"private-other"}, [goalResult]), null);
  assert.equal(resultForMessage(submitted, []), null);
  assert.equal(resultForMessage(submitted, [{...goalResult, work:[{...goalResult.work[0], state:contracts.WorkState.Ready}]}]), null);
  const failed = { ...submitted, event: { ...submitted.event, kind: contracts.GoalEventKind.Failed } };
  assert.equal(resultForMessage(failed, [goalResult]), null);
  assert.equal(resultForMessage(failed, [{...goalResult, work:[{...goalResult.work[0], state:contracts.WorkState.Failed, result:{succeeded:false, summary:"Recorded failure"}}]}]).result.summary, "Recorded failure");

  const { replyEdges } = moduleFrom(base + "collaboration-model.ts");
  const msg = (sequence, sender_id, reply_to = null, group_id = "a") => ({
    sequence,
    sender_id,
    reply_to,
    group_id,
    text: "Hello",
    accepted_ms: 1000,
  });
  assert.deepEqual(
    replyEdges([
      msg(1, "a"),
      msg(2, "b", 1),
      msg(3, "a", 1),
      msg(4, "c", 99),
      msg(5, "d", 1, "other"),
    ]),
    [{ from: "b", to: "a", sequence: 2 }],
  );
  assert.deepEqual(
    replyEdges([msg(2, "b", 1)]),
    [],
    "A missing parent is not proof of collaboration",
  );

  const goalFixture = (id) => ({input:{id, group_id:"a", title:id, objective:"Question", acceptance:"Evidence", scenario:"research", max_seconds:60}, state:contracts.GoalState.Active, revision:1, work:[{id:"work",person_id:"member",attempt:1,state:contracts.WorkState.Submitted,deadline_ms:null,instruction:"Review",result:{succeeded:true,summary:"Result",evidence:{pin:"one"}}}]});
  const goalPages = {"":{goals:[goalFixture("first")],next_after:"first"}, first:{goals:[goalFixture("second")],next_after:"second"}, second:{goals:[goalFixture("third")],next_after:null}};
  let holdGoals = null;
  const goals = moduleFrom(base + "goals.ts", {
    "/assets/shared/contracts.js":contracts, "./i18n.js":{...i18n,label:(_type,value)=>value}, "./dom.js":dom,
    "./people.js":{display:id=>id}, "./workspace.js":{revealResearch:()=>{}},
    "./api.js":{query:p=>new URLSearchParams(p).toString(),api:async url=>holdGoals ? holdGoals.promise : goalPages[new URL(url,"http://local").searchParams.get("after")]},
  }, {document:new EventTarget(),window:{clearTimeout,setTimeout}});
  goals.selectGoals({id:"a",kind:contracts.GroupKind.Conversation});
  await goals.loadGoals(); await goals.loadGoals(true);
  const secondCard = get("goal-list").children[1]; secondCard.open = true;
  secondCard.querySelectorAll("[data-evidence-key]")[0].open = true;
  await goals.loadGoals();
  assert.equal(goals.goalSnapshot().length,2,"Refresh must retain already loaded pages");
  assert(get("goal-list").children[1].open);
  assert(get("goal-list").children[1].querySelectorAll("[data-evidence-key]")[0].open,"Refresh must retain an open evidence disclosure");
  await goals.focusGoal("third");
  assert.equal(goals.goalSnapshot().length,3,"Focusing historical evidence must load its goal");
  assert(get("goal-list").children[2].open);
  holdGoals = deferred(); const staleGoals = goals.loadGoals(); goals.suspendGoals();
  holdGoals.resolve(goalPages[""]); await staleGoals;
  assert.equal(goals.goalSnapshot().length,0,"Old group fetches must not restore results");

  const browser = new EventTarget();
  let confirm = false,
    prompts = 0;
  browser.confirm = () => {
    prompts++;
    return confirm;
  };
  const guard = moduleFrom(
    base + "settings-guard.ts",
    { "./i18n.js": i18n },
    { window: browser },
  );
  const form = new Node("form");
  let resets = 0;
  form.reset = () => resets++;
  guard.trackSettings(form);
  guard.initSettingsGuard();
  form.dispatchEvent(new Event("change"));
  assert.equal(guard.allowSettingsLeave(), false);
  assert.equal(guard.hasUnsavedSettings(), true);
  const unload = new Event("beforeunload", { cancelable: true });
  browser.dispatchEvent(unload);
  assert(unload.defaultPrevented);
  confirm = true;
  assert(guard.allowSettingsLeave());
  assert.equal(resets, 1);
  form.dispatchEvent(new Event("input"));
  guard.savedSettings(form);
  assert(guard.allowSettingsLeave());
  assert.equal(prompts, 2);
  form.dispatchEvent(new Event("input"));
  form.isConnected = false;
  assert.equal(guard.hasUnsavedSettings(), false);

  let request = deferred();
  const sends = [];
  const messages = moduleFrom(
    base + "messages.ts",
    {
      "/assets/shared/contracts.js": contracts,
      "./i18n.js": i18n,
      "./api.js": {
        api: () => request.promise,
        change: (payload) => { sends.push(payload); return request.promise; },
        query: () => "",
      },
      "./dom.js": dom,
      "./people.js": { display: (id) => id },
      "./goals.js": { goalSnapshot: () => [], focusGoal: () => {} },
      "./message-results.js": { resultForMessage },
    },
    { document: new EventTarget(), crypto: require("node:crypto").webcrypto },
  );
  const a = {
      id: "a",
      kind: contracts.GroupKind.Conversation,
      archived: false,
    },
    b = { ...a, id: "b" };
  messages.initMessages();
  messages.resetMessages(a, [msg(1, "a")]);
  get("message-text").value = "unfinished";
  messages.suspendMessages();
  messages.resetMessages(b, [msg(8, "b", null, "b")]);
  assert.equal(get("message-text").value, "");
  messages.suspendMessages();
  messages.resetMessages(a, [msg(1, "a")]);
  assert.equal(
    get("message-text").value,
    "unfinished",
    "Switching groups must preserve each draft",
  );
  const history = get("older-messages").onclick();
  messages.suspendMessages();
  messages.resetMessages(b, [msg(8, "b", null, "b")]);
  request.resolve({ messages: [msg(0, "a")] });
  await history;
  assert.deepEqual(
    messages.messageSnapshot().map((m) => m.group_id),
    ["b"],
    "Late history cannot overwrite a different group",
  );
  messages.incoming(msg(10, "a", null, "a"));
  assert.equal(messages.messageSnapshot().length, 1);

  request = deferred();
  get("message-text").value = "one submission";
  const submit = () => get("composer").onsubmit({ preventDefault() {} });
  const sending = submit();
  messages.updateComposer(b);
  assert.equal(get("send-message").disabled, true, "SSE must not re-enable a pending send");
  await submit();
  assert.equal(sends.length, 1, "Repeated submissions must share one in-flight operation");
  request.reject(new Error("network failure"));
  await sending;
  assert.equal(get("message-text").value, "one submission");
  request = deferred();
  const retry = submit();
  assert.equal(sends[1].message.request_id, sends[0].message.request_id, "Retry must retain request identity");
  messages.suspendMessages();
  messages.resetMessages(a, [msg(1, "a")]);
  request.resolve({ message: msg(11, "b", null, "b") });
  await retry;
  assert.equal(get("message-text").value, "unfinished", "Late send must not clear another group's draft");
  assert.equal(messages.messageSnapshot()[0].group_id, "a");

  const pending = new Map();
  const chosen = [];
  const streams = [];
  class Stream {
    constructor() {
      this.handlers = {};
      streams.push(this);
    }
    close() {
      this.closed = true;
    }
    addEventListener(name, fn) {
      this.handlers[name] = fn;
    }
  }
  const conversation = moduleFrom(
    base + "conversation.ts",
    {
      "/assets/shared/contracts.js": contracts,
      "./i18n.js": i18n,
      "./api.js": {
        api: (url) => {
          const p = deferred();
          pending.set(url, p);
          return p.promise;
        },
        query: (fields) => fields.group_id,
      },
      "./dom.js": dom,
      "./directory.js": {
        setSelected: (id) => chosen.push(id),
        metrics: () => {},
        loadGroups: async () => {},
      },
      "./messages.js": {
        incoming: () => {},
        suspendMessages: () => {},
        resetMessages: () => {},
        updateComposer: () => {},
      },
      "./feedback.js": { busy: () => () => {} },
      "./goals.js": {
        selectGoals: () => {},
        suspendGoals: () => {},
        loadGoals: async () => {},
      },
      "./evolution.js": { selectEvolution: () => {} },
      "./collaboration.js": {
        selectCollaboration: () => {},
        refreshCollaboration: () => {},
      },
    },
    { EventSource: Stream },
  );
  const first = conversation.select(a),
    second = conversation.select(b);
  pending.get("/api/crystal/view?b").resolve({ group: b, messages: [] });
  await second;
  pending.get("/api/crystal/view?a").resolve({ group: a, messages: [] });
  await first;
  assert.equal(conversation.current.id, "b");
  assert.deepEqual(chosen, ["b"]);
  assert.equal(streams.length, 1);
  const again = conversation.select(b);
  await again;
  assert.equal(
    streams.length,
    1,
    "Returning to the active group must preserve its stream and draft",
  );
  const third = conversation.select(a);
  assert(streams[0].closed);
  streams[0].handlers.status({
    data: JSON.stringify({ group: b, metrics: {} }),
  });
  assert.equal(
    conversation.current,
    null,
    "Retired status handlers cannot restore an old group",
  );
  pending.get("/api/crystal/view?a").resolve({ group: a, messages: [] });
  await third;
  assert.equal(conversation.current.id, "a");
  console.log(
    "Verified reply evidence, unsaved settings, draft restoration, history and conversation request fences.",
  );
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
