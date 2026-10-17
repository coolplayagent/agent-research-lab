const assert = require("node:assert/strict"), fs = require("node:fs"), path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports = {}, globals = {}) {
  const module = { exports: {} };
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), "utf8"), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  new Function("module", "exports", "require", ...Object.keys(globals), code)(module, module.exports, id => {
    assert(Object.hasOwn(imports, id), id); return imports[id];
  }, ...Object.values(globals));
  return module.exports;
}
const webcrypto = require("node:crypto").webcrypto;
const insecureCrypto = Object.freeze({ getRandomValues: webcrypto.getRandomValues.bind(webcrypto) });
assert.equal(insecureCrypto.randomUUID, undefined);
const identity = load("web/shared/src/identity.ts", {}, { crypto: insecureCrypto, isSecureContext: false });
const format = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
const values = new Set(Array.from({ length: 128 }, () => identity.uuid()));
assert.equal(values.size, 128);
for (const value of values) assert.match(value, format);
assert.equal(insecureCrypto.randomUUID, undefined, "The helper must not patch the browser global");

const { MessageDrafts } = load("web/outcome-hub/src/message-drafts.ts", { "/assets/shared/identity.js": identity });
const drafts = new MessageDrafts(), draft = { text: "Message from an HTTP interface", reply: null };
const first = drafts.begin("group", draft);
assert.match(first.request_id, format);
drafts.finish("group");
assert.equal(drafts.begin("group", draft).request_id, first.request_id, "A response-loss retry preserves identity");
drafts.finish("group");
assert.notEqual(drafts.begin("group", { ...draft, text: "A changed message" }).request_id, first.request_id);

class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.attributes = {}; }
  append(...children) { this.children.push(...children); }
  setAttribute(key, value) { this.attributes[key] = value; }
  addEventListener() {}
}
const document = { createElement: tag => new Element(tag), body: new Element("body") };
const forms = load("web/outcome-hub/src/forms.ts", {
  "/assets/shared/identity.js": identity,
  "./settings-guard.js": { allowSettingsLeave: () => true, savedSettings: () => {} },
  "./feedback.js": { busy: () => () => {} }, "./i18n.js": { t: key => key },
}, { document });
const dialog = forms.dialog("Create an object");
assert.match(dialog.dialog.attributes["aria-labelledby"].slice("dialog-".length), format);
assert.equal(document.body.children.length, 1, "Dialog construction works without secure-context randomUUID");
console.log("Verified HTTP-compatible random identities, dialog creation and response-loss message retries without global patching.");
