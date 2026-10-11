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
    this.classes = new Set();
    this.classList = {
      add: (name) => this.classes.add(name),
      remove: (...names) => names.forEach(name => this.classes.delete(name)),
      toggle: (name, enabled) => enabled ? this.classes.add(name) : this.classes.delete(name),
    };
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
    const key = selector.includes("data-progress-key") ? "progressKey" : selector.includes("data-goal-id") ? "goalId" : selector.includes("data-evidence-key") ? "evidenceKey" : "sequence";
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
const i18n = { t: (key, params) => key === "messages.unseen" ? `${key}:${params.count}` : key };
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((r, j) => {
    resolve = r;
    reject = j;
  });
  return { promise, resolve, reject };
};
(async () => {
  const { contributionText } = moduleFrom(base + "contribution-text.ts");
  assert.equal(contributionText("Analyst: A finding", "Analyst"), "A finding");
  assert.equal(contributionText("@Analyst: A finding", "Analyst"), "A finding");
  assert.equal(contributionText("Reviewer: Another view", "Analyst"), "Reviewer: Another view");
  assert.equal(contributionText("My source is Analyst: example", "Analyst"), "My source is Analyst: example");
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

  const progressModule = moduleFrom(base + "task-progress.ts", {
    "/assets/shared/contracts.js": contracts, "./dom.js": dom,
    "./i18n.js": { ...i18n, label: (type, value) => `${type}.${value}` },
    "./people.js": { display: id => id }, "./message-results.js": { resultForMessage },
  });
  const { taskTimeline, progressSummary, progressCard } = progressModule;
  const event = (sequence, kind, attempt = 1, person_id = "member") => ({
    ...submitted, sequence, accepted_ms: 1000, sender_id: "system", text: "Original persisted text",
    event: { ...submitted.event, kind, attempt, person_id, title: "Research experience" },
  });
  const lifecycle = [event(1, contracts.GoalEventKind.Created, null, null), event(2, contracts.GoalEventKind.Claimed), event(3, contracts.GoalEventKind.Failed), event(4, contracts.GoalEventKind.Retried, 2), event(5, contracts.GoalEventKind.Claimed, 2)];
  const runningGoal = { ...goalResult, state: contracts.GoalState.Active, work: [{ ...goalResult.work[0], state: contracts.WorkState.Running, deadline_ms: null, result: null }] };
  const original = JSON.stringify(lifecycle);
  const compact = taskTimeline(lifecycle, [runningGoal]);
  assert.equal(compact.length, 1, "Repeated lifecycle updates share one progress card");
  assert.equal(compact[0].progress.events.length, 5);
  assert.equal(progressSummary(compact[0].progress.goal), "progress.running", "A retried attempt must supersede its historical failure");
  assert.equal(JSON.stringify(lifecycle), original, "Stored Unicode/user text is never rewritten");
  const currentFailure = { ...runningGoal, work: [{ ...runningGoal.work[0], state: contracts.WorkState.Failed }] };
  assert.equal(progressSummary(currentFailure), "progress.attention");
  assert.equal(progressSummary({ ...runningGoal, work: [{ ...runningGoal.work[0], deadline_ms: 10 }] }, 11), "progress.attention");
  assert.equal(progressSummary({ ...runningGoal, work: [{ ...runningGoal.work[0], state: contracts.WorkState.Ready }] }), "progress.waiting");
  assert.equal(progressSummary({ ...goalResult, state: contracts.GoalState.Active }), "progress.review");
  assert.equal(progressSummary({ ...goalResult, state: contracts.GoalState.Completed }), "GoalState.completed");
  assert.equal(progressSummary(undefined), "progress.unavailable");
  assert.equal(progressSummary({ ...runningGoal, work: [] }), "progress.unavailable");
  assert.equal(progressSummary({ ...runningGoal, work: [{ ...runningGoal.work[0], state: "future-unknown" }] }), "progress.unavailable");
  const separate = taskTimeline([...lifecycle, { ...lifecycle[0], sequence: 6, group_id: "other" }, { ...lifecycle[0], sequence: 7, event: { ...lifecycle[0].event, goal_id: "other-goal" } }], [runningGoal]);
  assert.equal(separate.length, 3);
  assert.equal(separate[1].progress.goal, undefined, "Do not attach another group's goal snapshot");
  const contribution = { ...event(6, contracts.GoalEventKind.Submitted, 2), sender_id: "member", text: "Actual contribution" };
  const ordinary = { sequence: 7, group_id: "team", sender_id: "operator", text: "User-owned content" };
  const withContributions = taskTimeline([...lifecycle, contribution, ordinary], []);
  assert.equal(withContributions[1].message, contribution, "Actual research stays visible, including older attempts");
  assert.equal(withContributions[2].message, ordinary);
  assert.equal(taskTimeline([event(6, contracts.GoalEventKind.Submitted, 2)], [goalResult]).length, 2, "Retain legacy result projections");
  assert.equal(taskTimeline([event(6, contracts.GoalEventKind.Failed, 2)], [currentFailure]).length, 1, "Failed attempts stay in progress history");
  let reviewed;
  const card = progressCard(compact[0].progress, true, id => reviewed = id);
  assert.equal(card.querySelectorAll("details[open][data-progress-key]").length, 1);
  assert.equal(card.querySelectorAll("[data-sequence]").length, 5);
  await card.children.at(-1).onclick(); assert.equal(reviewed, "goal");

  const { mentionText, mentionNames, resolveMentions } = moduleFrom(base + "mention-model.ts");
  const participants = [{id:"a",name:"Analyst"},{id:"b",name:"Research Lead"}];
  assert.deepEqual(mentionNames('mail@example.org @Analyst @"Research Lead"'),["Analyst","Research Lead"]);
  assert.deepEqual(resolveMentions('@Analyst @Analyst @"Research Lead"', participants).people.map(p=>p.id), ["a","b"]);
  assert.deepEqual(resolveMentions('@Outsider', participants).unresolved, ["Outsider"]);
  assert.deepEqual(resolveMentions('@Analyst', [...participants,{id:"c",name:"Analyst"}]).unresolved,["Analyst"]);
  assert.equal(mentionText(participants[1]), '@"Research Lead"');

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
    "./people.js":{display:id=>id}, "./workspace.js":{revealResearch:()=>{}}, "./contribution-text.js": { contributionText },
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
  const discussionSends = [];
  let discussionInput = null;
  const draftModule = moduleFrom(base + "message-drafts.ts", {}, { crypto: require("node:crypto").webcrypto });
  const messages = moduleFrom(
    base + "messages.ts",
    {
      "/assets/shared/contracts.js": contracts,
      "./i18n.js": i18n,
      "./api.js": {
        api: (url, payload) => { if (url === "/api/im/discussion") discussionSends.push(payload); return request.promise; },
        change: (payload) => { sends.push(payload); return request.promise; },
        query: () => "",
      },
      "./dom.js": dom,
      "./people.js": { display: (id) => id },
      "./goals.js": { goalSnapshot: () => [], focusGoal: () => {} },
      "./message-drafts.js": draftModule,
      "./message-view.js": { messageRow: () => new Node("article") },
      "./task-progress.js": progressModule,
      "./mentions.js": { initMentions: () => {}, insertMention: () => {}, prepareDiscussion: async () => discussionInput },
    },
    { document: new EventTarget(), CustomEvent: class extends Event { constructor(type, options) { super(type); this.detail = options.detail; } }, crypto: require("node:crypto").webcrypto },
  );
  const a = {
      id: "a",
      kind: contracts.GroupKind.Conversation,
      archived: false,
    },
    b = { ...a, id: "b" };
  messages.initMessages();
  messages.resetMessages({ ...a, id: "team" }, lifecycle);
  get("messages").querySelectorAll("[data-progress-key]")[0].open = true;
  messages.incoming(ordinary);
  assert.equal(get("messages").querySelectorAll("details[open][data-progress-key]").length, 1, "Live arrivals preserve an expanded task history");
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
  messages.suspendMessages();
  messages.resetMessages(a, [msg(1, "a")]);
  messages.suspendMessages();
  messages.resetMessages(b, [msg(8, "b", null, "b")]);
  request = deferred();
  const retry = submit();
  assert.equal(sends[1].message.request_id, sends[0].message.request_id, "Retry after navigation must retain request identity");
  messages.suspendMessages();
  messages.resetMessages(a, [msg(1, "a")]);
  request.resolve({ message: msg(11, "b", null, "b") });
  await retry;
  assert.equal(get("message-text").value, "unfinished", "Late send must not clear another group's draft");
  assert.equal(messages.messageSnapshot()[0].group_id, "a");
  messages.suspendMessages();
  messages.resetMessages(b, [msg(11, "b", null, "b")]);
  assert.equal(get("message-text").value, "", "Late success must clear its own saved draft");

  request = deferred();
  get("message-text").value = "in flight across navigation";
  const returningSend = submit();
  await Promise.resolve();
  messages.suspendMessages(); messages.resetMessages(a, [msg(1, "a")]);
  messages.suspendMessages(); messages.resetMessages(b, [msg(11, "b", null, "b")]);
  assert.equal(get("send-message").disabled, true, "Returning must retain the in-flight lock");
  const beforeRepeat = sends.length;
  await submit();
  assert.equal(sends.length, beforeRepeat);
  request.resolve({message:msg(12,"b",null,"b")}); await returningSend;
  assert.equal(get("message-text").value, "");

  request = deferred();
  const older = get("older-messages").onclick();
  request.resolve({messages:[msg(8,"b",null,"b")]}); await older;
  messages.incoming(msg(13,"b",null,"b"));
  messages.incoming(msg(13,"b",null,"b"));
  assert.equal(get("latest-messages").textContent,"messages.unseen:1","Replayed events must not increase the unseen count");
  assert.deepEqual(messages.messageSnapshot().map(m=>m.sequence),[8],"New arrivals must not replace the history being read");
  request = deferred();
  const latest = get("latest-messages").onclick();
  messages.incoming(msg(14,"b",null,"b"));
  request.resolve({messages:[msg(12,"b",null,"b")]}); await latest;
  assert(messages.messageSnapshot().some(m=>m.sequence===14),"Live events during a latest-page request must survive its older snapshot");
  assert.equal(get("latest-messages").textContent,"html.1c59875d3a");
  request = deferred();
  const staleHistory = get("older-messages").onclick();
  const oldPage = request;
  request = deferred();
  const freshLatest = get("latest-messages").onclick();
  request.resolve({messages:[msg(14,"b",null,"b")]}); await freshLatest;
  oldPage.resolve({messages:[msg(8,"b",null,"b")]}); await staleHistory;
  assert(messages.messageSnapshot().some(m=>m.sequence===14),"A late history page must not undo returning to latest");

  discussionInput = {scenario:"research", assignments:[{person_id:"member",instruction:"Initial instruction"}]};
  get("message-text").value = "@member discuss this";
  request = deferred();
  const discussion = submit(); await Promise.resolve();
  request.reject(new Error("lost discussion response")); await discussion;
  messages.suspendMessages(); messages.resetMessages(a,[msg(1,"a")]);
  messages.suspendMessages(); messages.resetMessages(b,[msg(14,"b",null,"b")]);
  discussionInput = {scenario:"collaboration", assignments:[]};
  request = deferred(); const repeatDiscussion = submit(); await Promise.resolve();
  assert.equal(discussionSends.length,2);
  assert.deepEqual(discussionSends[1],discussionSends[0],"Discussion retries must freeze both the message identity and its original goal input across navigation");
  request.resolve({message:msg(15,"b",null,"b")}); await repeatDiscussion;
  discussionInput = null;

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
      "./workspace.js": { syncControls() {} },
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
      "./mentions.js": { selectMentions: () => {} },
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
