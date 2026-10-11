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
  querySelectorAll() {
    return this.children.filter((node) => node.dataset.sequence);
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
