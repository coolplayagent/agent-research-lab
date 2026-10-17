const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
const source = fs.readFileSync(path.join(root, "web/outcome-hub/src/bootstrap.ts"), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
async function boot(local, hash, existingCookie = false) {
  const nodes = new Map(), calls = [], replaced = [];
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
  modules["./dom.js"] = { get, initDialogs:noOp, notice:(error,id) => { get(id).textContent = String(error); }, value:id => get(id).value };
  modules["./api.js"] = { api:async (url, body) => {
    calls.push({url,body});
    if (body) return {authenticated:true};
    if (local || existingCookie) return {authenticated:true,local_operator:local};
    throw new Error("Host operator login required");
  } };
  modules["./directory.js"].loadGroups = async () => { loaded++; };
  class Events { addEventListener() {} }
  await new AsyncFunction("module","exports","require","location","history","EventSource","document",code)(
    {exports:{}},{},id => { assert(modules[id],id); return modules[id]; },
    {hash,pathname:"/",reload:noOp}, {replaceState:(_state,_title,url) => replaced.push(url)}, Events, new EventTarget(),
  );
  return {nodes,calls,replaced,loaded};
}
(async () => {
  const local = await boot(true, "#access=obsolete-token");
  assert.equal(local.calls.length,1,"Local bootstrap never posts an obsolete credential");
  assert.equal(local.calls[0].body,undefined);
  assert.equal(local.nodes.get("login").hidden,true);
  assert.equal(local.nodes.get("workspace").hidden,false);
  assert.equal(local.nodes.get("logout").hidden,true);
  assert.deepEqual(local.replaced,["/#groups"]);
  assert.equal(local.loaded,1);
  const credential = await boot(false, "#access=valid-token",true);
  assert.equal(credential.calls[1].body.token,"valid-token","Credential mode preserves explicit link login");
  assert.equal(credential.loaded,1);
  const unauthenticated = await boot(false,"#groups");
  assert.equal(unauthenticated.loaded,0);
  assert(unauthenticated.nodes.get("login-status").textContent.includes("Host operator login required"));
  console.log("Verified local bootstrap, obsolete access fragments and credential mode compatibility.");
})().catch(error => { console.error(error); process.exit(1); });
