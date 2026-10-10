/** Check translation call sites with the same pinned parser used to build the UI. */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
const parse = (name, text) => ts.createSourceFile(name, text, ts.ScriptTarget.Latest, true);
const read = name => fs.readFileSync(path.join(root, name), "utf8");
function catalog(name) {
  const source = parse(name, read(name));
  const values = {};
  function visit(node) {
    if (ts.isPropertyAssignment(node) && ts.isStringLiteral(node.name) && ts.isStringLiteral(node.initializer)) {
      assert(!Object.hasOwn(values, node.name.text), `${name}: duplicate key ${node.name.text}`);
      values[node.name.text] = node.initializer.text;
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  assert(Object.keys(values).length > 0, `${name}: empty catalog`);
  return values;
}
function checkCalls(source, messages) {
  const errors = [];
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(source) === "t" && ts.isStringLiteral(node.arguments[0])) {
      const key = node.arguments[0].text;
      if (!Object.hasOwn(messages, key)) errors.push(`missing key ${key}`);
      else {
        const required = [...messages[key].matchAll(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g)].map(m => m[1]);
        const args = node.arguments[1];
        const actual = args && ts.isObjectLiteralExpression(args) ? args.properties.map(p => p.name.getText(source)) : [];
        for (const name of required) if (!actual.includes(name)) errors.push(`${key}: missing parameter ${name}`);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return errors;
}
(async () => {
  const { createI18n } = await import(pathToFileURL(path.join(root, "web/shared/dist/i18n.js")));
  const fixture = createI18n({ en: { greeting: "Hello {name}", "api.example": "Record {id}" } }, "unknown", "en");
  assert.equal(fixture.locale, "en");
  assert.equal(fixture.t("greeting", {name: "<script>user data</script>"}), "Hello <script>user data</script>");
  assert.equal(fixture.describe({code: "example", parameters: {id: 3}}), "Record 3");
  assert.equal(fixture.describe({code: "future_code"}), "future_code");
  assert.throws(() => fixture.t("missing"), /Missing translation/);
  assert.throws(() => fixture.t("greeting"), /Missing parameter/);
  assert.deepEqual(checkCalls(parse("fixture.ts", 't("greeting", {name: "A"})'), {greeting: "Hello {name}"}), []);
  assert.equal(checkCalls(parse("fixture.ts", 't("greeting", {}); t("missing")'), {greeting: "Hello {name}"}).length, 2);
  const shared = catalog("web/shared/src/locales/zh-CN.ts");
  const contracts = await import(pathToFileURL(path.join(root, "web/shared/dist/contracts.js")));
  for (const [domain, variants] of Object.entries(contracts)) {
    for (const value of Object.values(variants)) {
      const key = `${domain === "ApiNotice" ? "api" : domain}.${value}`;
      assert(Object.hasOwn(shared, key), `state lacks locale: ${key}`);
    }
  }
  const inventory = read("source_inventory.txt").trim().split("\n");
  let count = 0;
  for (const [directory, html] of [["crates/im-service/web", "crates/im-service/web/index.html"], ["app/src/dashboard/web", "app/src/dashboard/index.html"]]) {
    const messages = {...shared, ...catalog(`${directory}/src/locales/zh-CN.ts`)};
    for (const file of inventory.filter(p => p.startsWith(directory + "/src/") && p.endsWith(".ts") && !p.includes("/locales/"))) {
      const errors = checkCalls(parse(file, read(file)), messages);
      assert.equal(errors.length, 0, `${file}: ${errors.join(", ")}`);
      count++;
    }
    for (const [, key] of read(html).matchAll(/data-i18n(?:-(?:title|placeholder|aria-label))?="([^"]+)"/g)) {
      assert(Object.hasOwn(messages, key), `${html}: missing key ${key}`);
    }
  }
  assert(count >= 35, `incomplete frontend inventory: ${count}`);
  console.log(`Verified ${count} modules, HTML keys, state labels, interpolation and locale fallback.`);
})().catch(error => { console.error(error); process.exit(1); });
