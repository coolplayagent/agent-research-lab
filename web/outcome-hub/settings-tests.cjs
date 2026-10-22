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
  constructor(tag,text) { this.tag=tag;this.textContent=text;this.children=[];this.value='';this.hidden=false;this.isConnected=true; }
  append(...children) { for(const c of children) { c.parent=this;this.children.push(c); } }
  closest() { return this.parent; }
  close() {this.closed=true;}
  showModal() {}
  setAttribute() {}
  scrollIntoView() {}
  replaceChildren(...children) {this.children=[];this.append(...children);}
}
const contracts=load('web/shared/src/contracts.ts',{});
let fields,submit,latestBox;
const forms={
  element:(tag,text)=>new Node(tag,text),
  input:(value)=>Object.assign(new Node('input'),{value}),
  area:(value)=>Object.assign(new Node('textarea'),{value}),
  check:(checked)=>Object.assign(new Node('input'),{checked}),
  select:(options,value)=>Object.assign(new Node('select'),{options,value}),
  field:(title,node)=>{fields[title]=node;const label=new Node('label');label.append(node);return label;},
  dialog:()=>{fields={};return latestBox={form:new Node('form'),dialog:new Node('dialog'),status:new Node('p')}},
  finish:(box,save)=>{submit=save;},
};
let selected;
const common={ './forms.js':forms,'./settings-guard.js':{trackSettings(){},allowSettingsLeave(){return true;}},
  './i18n.js':{t:key=>key,label:(type,value)=>`${type}.${value}`},'/assets/shared/contracts.js':contracts };
const {editAdapter}=load('web/outcome-hub/src/adapter-forms.ts',{...common,'./binding-settings.js':{chooseBinding:id=>selected=id}});
const {editService}=load('web/outcome-hub/src/service-forms.ts',common);
const config={revision:1,services:[{id:'executor',kind:contracts.ServiceKind.Executor,enabled:true,endpoint:'/private/executor.sock',sandbox_id:'sandbox'},
  {id:'sandbox',kind:contracts.ServiceKind.Sandbox,enabled:true},{id:'off',kind:contracts.ServiceKind.Sandbox,enabled:false}],adapters:[],bindings:{}};
(async()=>{
  let saved;
  editAdapter(config,undefined,async next=>saved=next);
  fields['services.id'].value='codex';
  fields['services.enabled'].checked=true; // Hidden legacy checkbox cannot enable a native template.
  await submit();
  assert.equal(saved.adapters[0].agent,contracts.CodingAgent.Codex);
  assert.equal(saved.adapters[0].enabled,false);
  assert.deepEqual(saved.adapters[0].command,[]);
  assert.equal(selected,undefined, 'Saving an adapter must stay on the adapter page');
  editAdapter(config,undefined,async next=>saved=next);
  fields['services.agent'].value=contracts.CodingAgent.Custom;
  fields['services.id'].value='custom';fields['services.enabled'].checked=true;
  await assert.rejects(submit(),/adapter_command_required/);
  const old={...saved.adapters[0],enabled:true,command:['/trusted/bridge']};
  editAdapter({...config,adapters:[old]},old,async next=>saved=next);
  assert(fields['services.agent'].disabled);
  await submit(); assert.equal(saved.adapters[0].enabled,true);
  fields['services.limit'].value='60';await submit();assert.equal(saved.adapters[0].enabled,false);
  editService(config,config.services[0],async next=>saved=next);
  assert(fields['services.id'].readOnly);assert(fields['services.kind'].disabled);
  assert.deepEqual(fields['services.sandbox'].options.map(o=>o[0]),['','sandbox']);
  fields['services.kind'].value=contracts.ServiceKind.Storage;
  await submit();assert.equal(saved.services.find(s=>s.id==='executor').kind,contracts.ServiceKind.Executor);
  editService(config,config.services[0],async next=>saved=next);
  fields['services.sandbox'].value='';await assert.rejects(submit(),/noSandbox/);
  const html=fs.readFileSync(path.join(root,'web/outcome-hub/index.html'),'utf8');
  assert(html.includes('id="advanced-settings"'));
  assert(html.includes('<div id="adapter-list"></div>'));
  assert(!html.includes('id="advanced-settings" open'));
  assert(!html.includes('data-settings-panel="scenarios"'));
  assert(!html.includes('data-setting="scenarios"'));
  assert(!fs.readFileSync(path.join(root,'web/outcome-hub/src/settings.ts'),'utf8').includes('renderOutcomeSettings'));
  const container=new Node('div'), calls=[], timers=[];
  let records=[], refreshed=0;
  global.window={setTimeout:callback=>timers.push(callback)};
  global.document={dispatchEvent(){}};
  const operation={request:{request_id:'test-id',adapter_id:'codex'},state:contracts.AdapterTestOperationState.Running,started_unix_ms:Date.now(),steps:[]};
  const api=async (url,body,intent)=>{
    calls.push({url,body,intent});
    if(!body)return records;
    assert.equal(intent,'manage-services');
    if(url.endsWith('/cancel')) {records=[{...operation,state:contracts.AdapterTestOperationState.Cancelled}];return records[0];}
    assert.equal(url,'/api/v1/collaboration/adapter-tests');
    assert(!('person_id' in body));
    records=[{...operation,request:body}];return records[0];
  };
  const dependencies={...common,'./api.js':{api,ApiError:class extends Error{}},
    './settings-guard.js':{trackSettings(){},savedSettings(){},hasUnsavedSettings:()=>false},
    './dom.js':{get:()=>container,actions:(...nodes)=>{const row=new Node('div');row.append(...nodes);return row;},button:(text,onclick)=>Object.assign(new Node('button',text),{onclick})},
    './adapter-forms.js':{editAdapter(){}},'/assets/shared/identity.js':{uuid:()=> 'test-id'}};
  const progress=load('web/outcome-hub/src/adapter-progress.ts',dependencies);
  const {renderAdapters}=load('web/outcome-hub/src/adapter-settings.ts',{...dependencies,'./adapter-progress.js':progress});
  const nodes=n=>[n,...n.children.flatMap(nodes)];
  const flush=async()=>{for(let i=0;i<8;i++)await Promise.resolve();};
  renderAdapters({...config,adapters:[{id:'codex',agent:contracts.CodingAgent.Codex,enabled:false}]},async()=>{},async()=>refreshed++);
  await flush();
  assert.equal(calls.filter(c=>c.body).length,0,'Rendering must not invoke a model');
  nodes(container).find(n=>n.textContent==='adapterTest.start').onclick();
  assert(!nodes(latestBox.form).some(n=>n.textContent==='binding.person'));
  await latestBox.form.onsubmit({preventDefault(){}});await flush();
  assert(latestBox.dialog.closed);
  assert.equal(calls.filter(c=>c.body).length,1);
  assert.equal(container.children[0].className,'cards');
  assert.equal(container.children.at(-1).className,'binding-progress-list','Logs follow all adapter controls');
  await nodes(container).find(n=>n.textContent==='adapterTest.cancel').onclick();
  assert(calls.at(-1).url.endsWith('/test-id/cancel'));
  records=[{...operation,state:contracts.AdapterTestOperationState.Succeeded,configuration_revision:2}];
  timers.pop()();await flush();
  assert.equal(refreshed,1,'Success refreshes the saved enabled configuration');
  assert(calls.every(c=>!c.url.includes('/bindings')));
  console.log('Settings behavior verified');
})().catch(e=>{console.error(e);process.exitCode=1;});
