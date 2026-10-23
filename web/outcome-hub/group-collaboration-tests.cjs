const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const ts = require(process.argv[2]), root = path.join(process.env.TEST_SRCDIR, process.env.TEST_WORKSPACE);
function load(file, imports = {}, globals = {}) {
  const module = { exports: {} }, source = fs.readFileSync(path.join(root,file),'utf8');
  const code = ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText;
  new Function('module','exports','require',...Object.keys(globals),code)(module,module.exports,id=>{assert(Object.hasOwn(imports,id),id);return imports[id];},...Object.values(globals));return module.exports;
}
class Node { constructor(tag,text=''){this.tag=tag;this.textContent=text;this.children=[];this.hidden=false;this.value='';this.dataset={};} append(...n){this.children.push(...n);} replaceChildren(...n){this.children=n;} setAttribute(){} querySelectorAll(){return [];} get childElementCount(){return this.children.length;} }
const nodes=new Map(),get=id=>{if(!nodes.has(id))nodes.set(id,new Node('div'));return nodes.get(id);};
const dom={get,el:(tag,text)=>new Node(tag,text),button:(text,onclick)=>Object.assign(new Node('button',text),{onclick}),notice(){}};
const contracts=load('web/shared/src/contracts.ts'),i18n={t:key=>key,label:(domain,key)=>`${domain}.${key}`};
const pending=()=>{let resolve;return {promise:new Promise(r=>resolve=r),resolve:v=>resolve(v)}};
(async()=>{
  const actualDom=load('web/outcome-hub/src/dom.ts',{'./i18n.js':i18n,'./feedback.js':{}});
  assert.equal(actualDom.presence(contracts.Presence.Offline,true),'members.archived');
  assert.equal(actualDom.presence(contracts.Presence.Online,false),'Presence.online');
  assert.equal(actualDom.presence(contracts.Presence.Busy,false),'Presence.busy');
  const requests=[],location={hash:'#groups'},gate=pending();
  const task=(id,group)=>({input:{id,group_id:group,intent:id},title:id,state:contracts.TaskState.Planning,kind:contracts.TaskKind.Outcome,revision:1,nodes:[],deliveries:[],events:[]});
  const tasks=load('web/outcome-hub/src/tasks.ts',{
    '/assets/shared/identity.js':{uuid:()=> 'id'},'/assets/shared/contracts.js':contracts,'./api.js':{query:v=>new URLSearchParams(v).toString(),api:async url=>{requests.push(url);return requests.length===1?gate.promise:{tasks:[task('second','b')],next_after:null};}},
    './dom.js':dom,'./forms.js':{},'./i18n.js':i18n,'./conversation.js':{current:null},'./task-render.js':{taskCard:(t,open)=>Object.assign(new Node('article',t.title),{onclick:open}),taskContent:()=>new Node('div')}
  },{location,document:{hidden:false},setInterval(){}});
  tasks.selectTasks({id:'a',archived:false});tasks.selectTasks({id:'b',archived:false});gate.resolve({tasks:[task('old','a')],next_after:null});await new Promise(setImmediate);
  assert.equal(get('task-list').children.length,0,'A stale group response must not populate the new board');
  await tasks.loadTasks();assert(requests.every(url=>url.includes('group_id=')));assert.equal(get('task-list').children[0].textContent,'second');
  get('task-list').children[0].onclick();assert.equal(location.hash,'groups/b/tasks/second');
  tasks.selectTasks(null);assert.equal(get('task-detail-content').children.length,0);
  const messages=load('web/outcome-hub/src/message-view.ts',{'/assets/shared/contracts.js':contracts,'./i18n.js':i18n,'./dom.js':dom,'./avatar.js':{},'./people.js':{display:id=>id},'./contribution-text.js':{},'./message-results.js':{}});
  let replied=false;const row=messages.messageRow({sequence:2,group_id:'b',sender_id:'operator',text:'What audience?',activity:{has_content:true,kind:contracts.TaskEventKind.Clarification,task_id:'intent',title:'Draft',person_id:null}},new Map(),[],{reply(){replied=true;}});
  const footer=row.children[0].children[2];assert.equal(footer.children[1].href,'#groups/b/tasks/intent');footer.children[0].onclick();assert(replied);
  const html=fs.readFileSync(path.join(root,'web/outcome-hub/index.html'),'utf8');assert(!html.includes('data-page="tasks"'));assert(html.indexOf('id="tasks-page"')>html.indexOf('id="goals-page"'));
  console.log('Group-scoped board, stale response isolation and replyable task chat passed');
})().catch(e=>{console.error(e);process.exit(1)});
