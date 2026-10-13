"""Real CLI/owner integration; only GitHub/fetch acquisition uses isolated fixtures."""
import hashlib,json,os,pathlib,subprocess,sys,tempfile,time
cli,owner=sys.argv[1:]
with tempfile.TemporaryDirectory(prefix='native-batch-cli-') as directory:
 root=pathlib.Path(directory);repo=root/'workspace'/'agent-research-lab';repo.mkdir(parents=True)
 def run(args,cwd=repo,env=None,timeout=45):
  return subprocess.run(args,cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout)
 def git(*args):
  result=run(['/usr/bin/git',*args]);assert result.returncode==0,result.stderr;return result.stdout.decode().strip()
 git('init','-q','-b','main');git('config','user.name','fixture');git('config','user.email','fixture@example.invalid')
 for name in ['research/insights.md','knowledge/index.md','knowledge/software/ai-sdlc/index.md','knowledge/methodology/evidence-method.md']:
  path=repo/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('Owned freshness acquisition fixture.\n')
 git('add','.');git('-c','commit.gpgsign=false','commit','-qm','owned fixture');commit=git('rev-parse','HEAD')
 git('remote','add','origin','https://github.com/coolplayagent/workflow-cli.git')
 bindings=root/'boundaries';bindings.mkdir();ledger=root/'acquisition.jsonl'
 gh=bindings/'gh';gh.write_text('#!/usr/bin/python3\n'+f'''import json,sys
with open({str(ledger)!r},'a') as f:f.write(json.dumps(sys.argv[1:])+'\\n')
a=sys.argv[1:]
if a[:2]==['repo','view']:v={{'nameWithOwner':a[2],'defaultBranchRef':{{'name':'main'}}}}
elif a[0]=='api' and a[1].endswith('/releases/latest'):v={{'draft':False,'prerelease':False,'id':1,'tag_name':'v-fixture'}}
elif a[0]=='api' and '/commits/' in a[1]:v={{'sha':{commit!r}}}
else:sys.exit(3)
print(json.dumps(v))
''');gh.chmod(0o700)
 fetch=bindings/'git';fetch.write_text('#!/usr/bin/python3\n'+f'''import os,sys
args=sys.argv[1:]
if args and args[0]=='fetch':args=['update-ref',args[-1].split(':')[-1],{commit!r}]
os.execv('/usr/bin/git',['/usr/bin/git']+args)
''');fetch.chmod(0o700)
 skill=root/'installed-fixture';skill.mkdir();(skill/'SKILL.md').write_text('Explicit acquisition fixture; not a published skill.\n')
 digest=run([cli,'skill-digest',str(skill)],cwd=root);assert digest.returncode==0,digest.stderr
 tree=json.loads(digest.stdout)['result']['sha256'];runtime_sha=hashlib.sha256(pathlib.Path(owner).read_bytes()).hexdigest()
 names=['workflow-cli','relay-knowledge','into-markdown','qualitygate-cli','computer-use-cli','relay-memory','repo-sandbox']
 manifest=root/'skills.json';manifest.write_text(json.dumps({'schema_version':1,'skills':[{'name':name,'repository':'coolplayagent/workflow-cli','installed_path':str(skill),'runtime_path':owner,'tree_sha256':tree,'runtime_sha256':runtime_sha,'release_tag':'v-fixture','release_id':1,'source_commit':None,'checked_at':'2026-10-11T00:00:00Z'} for name in names]}))
 config=root/'config.toml'
 q=lambda v:json.dumps(str(v))
 config.write_text(f'''schema_version=1
workspace={q(root/'workspace')}
state_dir={q(root/'state')}
superpod={q(repo)}
codex="never-launched"
workflow={q(owner)}
daily_seconds=3600
max_agents=8
task_timeout_seconds=20
require_latest=true
skills_manifest={q(manifest)}
[models]
research="fixture"
review="fixture"
implement="fixture"
'''+''.join(f'\n[tools.{name}]\nbinary={q(owner)}\nrepository={q(repo)}\nprobe=[]\n' for name in names))
 env=os.environ.copy();env['PATH']=str(bindings)+':'+env['PATH']
 task={'id':'native-cli-task','role':'research','repository':'superpod','prompt':'Inspect the owned fixture; no execution requested.'}
 request=root/'batch.json';request.write_text(json.dumps({'schema_version':1,'batch_id':'cli-fixture','tasks':[task]}))
 started=time.monotonic();first=run([cli,'--config',str(config),'enqueue-batch',str(request),'--max-seconds','60'],env=env,timeout=90)
 assert first.returncode==0,(first.stdout,first.stderr)
 first_json=json.loads(first.stdout);assert first_json['ok'] is True and first_json['result']['state']=='complete'
 assert first_json['result']['entries'][0]['state']=='workflow_confirmed'
 job=root/'state/jobs/native-cli-task.json';old=job.read_bytes();plan=root/'state/private/enqueue-batches/cli-fixture/plan.json';old_plan=plan.read_bytes()
 # Ordinary existing replay must not enter collect and keeps its historic request.
 ordinary=root/'task.json';ordinary.write_text(json.dumps(task));before=ledger.read_bytes()
 replay=run([cli,'--config',str(config),'enqueue',str(ordinary)],env=env);assert replay.returncode==0,replay.stderr
 assert ledger.read_bytes()==before and job.read_bytes()==old
 # Same canonical typed request with different raw whitespace/default representation.
 task.update({'write':False,'exploratory':False,'dependencies':[],'required_tools':[]})
 request.write_text(json.dumps({'schema_version':1,'batch_id':'cli-fixture','tasks':[task]},indent=2))
 replay=run([cli,'--config',str(config),'enqueue-batch',str(request),'--max-seconds','60'],env=env,timeout=90)
 assert replay.returncode==0,replay.stderr
 assert job.read_bytes()==old and plan.read_bytes()==old_plan
 assert ledger.read_bytes()!=before
 # Eight ordinary calls use eight native collections; one eight-member batch uses one.
 def collection_count():return sum(a[:1]==['api'] and a[1].endswith('/releases/latest') for a in (json.loads(line) for line in ledger.read_text().splitlines()))//len(names)
 batch_tasks=[{'id':f'batch-parity-{i}','role':'research','repository':'superpod','prompt':f'Owned parity {i}','max_attempts':2,'depth':1,'use_memory':False,'exploratory':i%2==0} for i in range(8)]
 parity=root/'parity.json';parity.write_text(json.dumps({'schema_version':1,'batch_id':'parity','tasks':batch_tasks}));count=collection_count()
 result=run([cli,'--config',str(config),'enqueue-batch',str(parity),'--max-seconds','60'],env=env,timeout=90)
 assert result.returncode==0,result.stderr
 assert collection_count()-count==1
 count=collection_count()
 for i,t in enumerate(batch_tasks):
  t=dict(t);t['id']=f'ordinary-parity-{i}';ordinary.write_text(json.dumps(t))
  result=run([cli,'--config',str(config),'enqueue',str(ordinary)],env=env);assert result.returncode==0,result.stderr
  registered=json.loads((root/f'state/jobs/ordinary-parity-{i}.json').read_text());batched=json.loads((root/f'state/jobs/batch-parity-{i}.json').read_text())
  assert registered['task']['id']==t['id'];registered['task']['id']=batched['task']['id'];assert registered['task']==batched['task']
  for key in ['model','source_commit','superpod_commit','config_digest']:assert registered[key]==batched[key]
 assert collection_count()-count==8
 # Request conflict and FIFO input are bounded failures with no acquisition.
 before=ledger.read_bytes();task['prompt']='conflicting request';request.write_text(json.dumps({'schema_version':1,'batch_id':'cli-fixture','tasks':[task]}))
 failed=run([cli,'--config',str(config),'enqueue-batch',str(request)],env=env)
 rejected=json.loads(failed.stderr);assert failed.returncode!=0 and rejected['ok'] is False and rejected['code']=='request_conflict',rejected
 fifo=root/'fifo';os.mkfifo(fifo);fifo_result=run([cli,'--config',str(config),'enqueue-batch',str(fifo),'--max-seconds','1'],env=env,timeout=3)
 assert fifo_result.returncode!=0 and json.loads(fifo_result.stderr)['code']=='invalid_request'
 assert ledger.read_bytes()==before and job.read_bytes()==old and plan.read_bytes()==old_plan
 bad=root/'bad-task.json';bad.write_text('{}');ordinary_error=run([cli,'--config',str(config),'enqueue',str(bad)],env=env)
 assert ordinary_error.returncode!=0 and set(json.loads(ordinary_error.stderr))=={'ok','error'}
 assert not (root/'state/runs').exists()
 print(json.dumps({'status':'pass','source_commit':commit,'cli_sha256':hashlib.sha256(pathlib.Path(cli).read_bytes()).hexdigest(),'workflow_sha256':runtime_sha,'elapsed_seconds':time.monotonic()-started,'first_registration':first_json['result'],'limits':['GitHub and fetch use owned acquisition boundary fixtures; no claim of live remote freshness.','No model process, lease or controller was started.']}))
