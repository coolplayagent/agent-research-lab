const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
const source = fs.readFileSync(path.join(root, "web/outcome-hub/src/bootstrap.ts"), "utf8");
const html = fs.readFileSync(path.join(root, "web/outcome-hub/index.html"), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
async function boot(hash) {
  const nodes = new Map(), replaced = [];
  const get = id => {
    if (!nodes.has(id)) nodes.set(id, { hidden: id === "workspace", value: "" });
    return nodes.get(id);
  };
  let loaded = 0;
  const noOp = () => {};
  const modules = {};
  for (const match of source.matchAll(/import\s*\{([^}]+)\}\s*from\s*"([^"]+)"/g)) {
    modules[match[2]] ??= {};
    for (const name of match[1].split(",").map(x => x.trim())) modules[match[2]][name] = noOp;
  }
  modules["./dom.js"] = { get, initDialogs:noOp, notice:noOp };
  modules["./directory.js"].loadGroups = async () => { loaded++; };
  class Events { addEventListener() {} }
  await new AsyncFunction("module","exports","require","location","history","EventSource","document",code)(
    {exports:{}},{},id => { assert(modules[id],id); return modules[id]; },
    {hash,pathname:"/"}, {replaceState:(_state,_title,url) => replaced.push(url)}, Events, new EventTarget(),
  );
  return {nodes,replaced,loaded};
}
(async () => {
  assert(!html.includes('id="login-form"'));
  assert(!html.includes('id="login-token"'));
  assert(!html.includes('id="logout"'));
  assert(!html.includes('agent-research-lab'));
  const direct = await boot("#groups");
  assert.equal(direct.nodes.get("workspace").hidden,false);
  assert.equal(direct.loaded,1);
  assert.deepEqual(direct.replaced,[]);
  const legacy = await boot("#access=obsolete-token");
  assert.equal(legacy.nodes.get("workspace").hidden,false);
  assert.equal(legacy.loaded,1);
  assert.deepEqual(legacy.replaced,["/#groups"]);
  console.log("Verified direct workspace bootstrap and legacy access-fragment cleanup.");
})().catch(error => { console.error(error); process.exit(1); });
