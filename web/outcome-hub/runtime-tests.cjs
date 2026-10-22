const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports) {
  const code = ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
  const module = {exports:{}};
  new Function('module','exports','require',code)(module,module.exports,id=>{assert(id in imports,id);return imports[id]});
  return module.exports;
}
class Node {
  constructor(tag,text='') { this.tag=tag; this.textContent=text; this.children=[]; }
  append(...nodes) { this.children.push(...nodes); }
  replaceChildren(...nodes) { this.children=nodes; }
  setAttribute() {}
}
const contracts=load('web/shared/src/contracts.ts',{});
const messages={...load('web/shared/src/locales/zh-CN.ts',{}).messages,...load('web/outcome-hub/src/locales/zh-CN.ts',{}).messages};
const t=key=>{assert(Object.hasOwn(messages,key),key);return messages[key];};
const element=(tag,text)=>new Node(tag,text), viewRoot=element('div');
const actions=(...nodes)=>{const node=element('div');node.append(...nodes);return node;};
let calls=[], failure=false, refreshed=0, finish;
const {renderExecutionStatus}=load('web/outcome-hub/src/execution-status.ts',{
  '/assets/shared/contracts.js':contracts,'./dom.js':{get:()=>viewRoot,actions},'./forms.js':{element},
  './i18n.js':{t,label:(type,value)=>t(type+'.'+value)},
  './api.js':{api:async(...args)=>{calls.push(args);await new Promise(resolve=>finish=resolve);if(failure)throw new Error('prepare failed');}},
});
function descendants(node) { return [node,...node.children.flatMap(descendants)]; }
(async()=>{
  const view={configuration:{revision:17,services:[{endpoint:'/private/socket'}],adapters:[],bindings:{},binding_evidence:{}},local_runtime:[
    {component:contracts.LocalRuntimeComponent.Storage,state:contracts.LocalRuntimeState.Ready,packaged:true,can_prepare:false},
    {component:contracts.LocalRuntimeComponent.Execution,state:contracts.LocalRuntimeState.Standby,packaged:true,can_prepare:true},
    {component:contracts.LocalRuntimeComponent.Knowledge,state:contracts.LocalRuntimeState.Missing,packaged:false,can_prepare:false},
  ]};
  renderExecutionStatus(view,[],async()=>{refreshed++;});
  let nodes=descendants(viewRoot);
  assert.equal(nodes.filter(n=>n.tag==='article').length,3);
  assert(!nodes.some(n=>n.textContent.includes('/private/socket')));
  assert(nodes.some(n=>n.tag==='a'&&n.textContent===t('runtime.download')));
  assert(nodes.some(n=>n.textContent===t('runtime.agentHelp')));
  const button=nodes.find(n=>n.tag==='button');
  const pending=button.onclick();assert.equal(button.disabled,true);
  assert.deepEqual(calls[0],['/api/v1/collaboration/services/prepare',{revision:17},'manage-services',45000]);
  finish();await pending;assert.equal(refreshed,1);
  renderExecutionStatus(view,[],async()=>{});nodes=descendants(viewRoot);failure=true;
  const retry=nodes.find(n=>n.tag==='button');const rejected=retry.onclick();finish();await rejected;
  assert.equal(retry.disabled,false);assert(nodes.some(n=>n.textContent.includes('prepare failed')));
  console.log('Local runtime cards, revision fencing, pending state, errors and retry verified');
})().catch(e=>{console.error(e);process.exitCode=1;});
