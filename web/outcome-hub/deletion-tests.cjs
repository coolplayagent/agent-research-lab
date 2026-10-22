const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.argv[2]);
const root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports = {}, globals = {}) {
  const code = ts.transpileModule(fs.readFileSync(path.join(root, file), 'utf8'), { compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022} }).outputText;
  const module = {exports:{}};
  new Function('module','exports','require',...Object.keys(globals),code)(module,module.exports,id=>{assert(Object.hasOwn(imports,id),id);return imports[id];},...Object.values(globals));
  return module.exports;
}
class Node {
  constructor(tag, text = '') { this.tag=tag; this.textContent=text; this.children=[]; this.dataset={}; }
  append(...nodes) {this.children.push(...nodes);}
  setAttribute() {}
}
const text = node => [node.textContent,...node.children.map(text)].join(' ');
const contracts = load('web/shared/src/contracts.ts');
const i18n = {t:key=>key,label:(domain,value)=>domain+'.'+value};
(async()=>{
  let approved=false, calls=[], outcome={code:contracts.DeletionCode.Deleted,references:[]};
  const deletion=load('web/outcome-hub/src/deletion.ts', {'/assets/shared/contracts.js':contracts,'./i18n.js':i18n,'./api.js':{change:async body=>{calls.push(body);return outcome;}}}, {confirm:()=>approved});
  const target={kind:'group',id:'room',revision:5};
  assert.equal(await deletion.deleteObject(target,'Room'),false);
  assert.equal(calls.length,0);
  approved=true;
  assert.equal(await deletion.deleteObject(target,'Room'),true);
  assert.deepEqual(calls[0],{operation:'delete',target});
  outcome={code:contracts.DeletionCode.EvidenceReferences,references:['version-1']};
  await assert.rejects(()=>deletion.deleteObject(target,'Room'),/evidence_references.*\nversion-1/);
  const resultModule=load('web/outcome-hub/src/message-results.ts', {'/assets/shared/contracts.js':contracts});
  const view=load('web/outcome-hub/src/message-view.ts', {
    '/assets/shared/contracts.js':contracts,'./i18n.js':i18n,
    './dom.js':{el:(tag,value)=>new Node(tag,value),button:(value,onclick)=>Object.assign(new Node('button',value),{onclick})},
    './avatar.js':{avatar:()=>new Node('span')},'./people.js':{display:id=>id},
    './contribution-text.js':{contributionText:value=>value},'./message-results.js':resultModule,
  });
  const event={kind:contracts.GoalEventKind.Failed,goal_id:'goal',work_id:'work',person_id:'worker',attempt:1,title:'Discussion'};
  const message={sequence:2,group_id:'group',sender_id:'operator',text:'work_failed',accepted_ms:1,event};
  const summary='Complete answer '.repeat(120);
  const goal={input:{id:'goal',group_id:'group'},work:[{id:'work',person_id:'worker',attempt:1,state:contracts.WorkState.Failed,result:{summary,succeeded:false}}]};
  const actions={mention(){},reply(){},locate(){},review(){}};
  const failed=view.messageRow(message,new Map(),[goal],actions);
  assert(text(failed).includes(summary),'Failed answers must display their full recorded content');
  assert(text(failed).includes('WorkState.failed'));
  const newer={...goal,work:[{...goal.work[0],attempt:2}]};
  assert(!text(view.messageRow(message,new Map(),[newer],actions)).includes(summary),'A retry must not attach the wrong attempt result');
  const deleted={...message,event:undefined,deleted:true,text:''};
  assert.equal(text(view.messageRow(deleted,new Map(),[],actions)).trim(),'deletion.deletedAnnouncement');
  const reply={...message,event:undefined,text:'Reply',reply_to:deleted.sequence,sequence:3};
  assert(text(view.messageRow(reply,new Map([[deleted.sequence,deleted]]),[],actions)).includes('deletion.deletedAnnouncement'));
  const html=fs.readFileSync(path.join(root,'web/outcome-hub/index.html'),'utf8');
  assert(!html.includes('href="#applications"'));
  assert(!html.includes('id="applications-page"'));
  assert(html.includes('id="delete-group"'));
  console.log('Verified deletion confirmation, blocked references, tombstones and current-attempt result presentation.');
})().catch(error=>{console.error(error);process.exit(1);});
