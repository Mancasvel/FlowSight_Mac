"""Live disposable-account integration suite. Never publishes; fixtures are removed."""
import json,os,pathlib,secrets,urllib.request,urllib.error
URL=os.environ['FLOWSIGHT_SUPABASE_URL']
admin=os.environ['FLOWSIGHT_SUPABASE_SERVICE_ROLE_KEY']
anon=os.environ['FLOWSIGHT_SUPABASE_ANON_KEY']

def call(path,data=None,token=admin,method=None):
 headers={'apikey':anon,'Authorization':'Bearer '+token,'Content-Type':'application/json'}
 request=urllib.request.Request(URL+path,data=json.dumps(data).encode() if data is not None else None,headers=headers,method=method)
 try:
  result=urllib.request.urlopen(request,timeout=55);body=result.read();return result.status,json.loads(body) if body else None
 except urllib.error.HTTPError as error:
  body=error.read()
  try:return error.code,json.loads(body)
  except ValueError:return error.code,{'error':'Non-JSON response'}
def api(token,action,**kwargs):return call('/functions/v1/build-in-public',{'action':action,**kwargs},token)
def make_user():
 email='flowsight-bip-test-'+secrets.token_hex(5)+'@example.invalid';password=secrets.token_urlsafe(30)
 status,user=call('/auth/v1/admin/users',{'email':email,'password':password,'email_confirm':True});assert status==200,(status,user)
 status,session=call('/auth/v1/token?grant_type=password',{'email':email,'password':password},anon);assert status==200,status
 return user['id'],session['access_token']
users=[];results=[]
def ok(label):
 results.append(label)
 print('PASS:',label,flush=True)
try:
 for _ in range(2):users.append(make_user())
 a,b=users[0][1],users[1][1]
 assert api('invalid','studio')[0]==401
 assert api(a,'studio')[0]==200
 ok('Authentication and live studio')
 evidence={'source_key':'synthetic-live-test','title':'Synthetic progress','summary':'Synthetic evidence for a test, not real user work.','occurred_on':'2026-10-12','kind':'progress','signals':{'observations':8,'recorded_seconds':600,'days':2,'topic':'Synthetic topic','rationale':'Useful synthetic example','angle':'Decision'}}
 assert api(a,'import',evidence=[evidence])[0]==200
 eid=api(a,'studio')[1]['evidence'][0]['id']
 assert api(a,'studio')[1]['evidence'][0]['signals']==evidence['signals']
 prefs={'project_name':'Synthetic project','locale':'en','timezone':'UTC','cadence':'weekly','excluded_terms':['secret-client'],'voice':'Synthetic voice'}
 assert api(a,'settings',settings=prefs)[0]==200
 assert api(a,'direction',text='Product decisions for founders')[0]==200
 saved=api(a,'studio')[1]['settings']
 assert all(saved[k]==v for k,v in prefs.items()) and saved['topic_direction']=='Product decisions for founders'
 assert api(b,'studio')[1]['settings']['topic_direction']==''
 assert api(a,'direction',text='x'*1001)[0]==400
 # Restore Spanish output for the subsequent prompt contract.
 prefs['locale']='es'
 assert api(a,'settings',settings=prefs)[0]==200
 assert api(a,'studio')[1]['settings']['topic_direction']=='Product decisions for founders'
 ok('Signals persist; direction preserves preferences, owner boundaries and length limits')
 assert api(b,'evidence',id=eid,patch={'state':'selected'})[0]==404
 assert api(a,'evidence',id=eid,patch={'state':'selected','confirmed':True})[0]==200
 ok('Cross-owner evidence changes rejected')
 profiles=[{'channel':'linkedin','name':'Synthetic author','handle':'@synthetic','style':'technical'}, {'channel':'x','name':'Synthetic author','handle':'@synthetic','style':'thread'}]
 for profile in profiles:assert api(a,'profile',profile=profile)[0]==200
 assert api(a,'profile',profile=profiles[0])[0]==200
 state=api(a,'studio')[1];assert len(state['profiles'])==2
 pid=next(p['id'] for p in state['profiles'] if p['channel']=='x')
 assert api(b,'profile',profile={**profiles[1],'id':pid})[0]==404
 assert api(a,'profile',profile={**profiles[1],'style':'technical'})[0]==400
 assert call('/rest/v1/bip_profiles?select=id',token=b)[1]==[]
 assert call('/rest/v1/bip_profiles',profiles[1],a)[0] in (401,403)
 ok('Profiles independent of Buffer; owner RLS and valid network styles')
 import uuid
 request={'batch_key':str(uuid.uuid4()),'evidence_ids':[eid],'targets':[{'channel':'x','style':'thread','profile_id':pid},{'channel':'instagram','style':'carousel','profile_id':None}]}
 assert api(a,'briefs',request=request)[0]==200
 assert api(a,'briefs',request=request)[0]==200
 state=api(a,'studio')[1];assert len(state['briefs'])==2
 brief=next(x for x in state['briefs'] if x['channel']=='x');bid=brief['id']
 assert '@synthetic' in brief['prompt'] and 'Hilo de 3 a 5' in brief['prompt']
 assert api(b,'briefs',request=request)[0]==404
 assert api(a,'briefs',request={**request,'targets':[{'channel':'linkedin','style':'technical','profile_id':pid}]})[0]==404
 assert call('/rest/v1/bip_briefs?select=id',token=b)[1]==[]
 ok('Persistent per-network briefs; idempotent and owner/network protected')
 assert api(a,'edit_brief',id=bid,revision=1,prompt=brief['prompt']+'\nSynthetic preference.')[0]==200
 assert api(a,'edit_brief',id=bid,revision=1,prompt='Stale')[0]==409
 assert api(b,'edit_brief',id=bid,revision=2,prompt='Foreign')[0]==409
 ok('Prompt revision protects concurrent edits')
 code,assistant=api(a,'assistant_token');assert code==200
 def mcp(method,params=None):return call('/functions/v1/build-in-public-assistant',{'jsonrpc':'2.0','id':1,'method':method,'params':params or {}},assistant['token'])
 def tool(name,args=None):return mcp('tools/call',{'name':name,'arguments':args or {}})[1]['result']
 def context():return json.loads(tool('get_content_context')['content'][0]['text'])
 assert mcp('initialize')[0]==200
 exposed=mcp('tools/list')[1]['result']['tools'];assert {t['name'] for t in exposed}=={'get_content_context','save_content_drafts','set_content_direction'}
 assert len(context()['briefs'])==2
 assert not tool('set_content_direction',{'text':'Lessons about product design'}).get('isError')
 assert context()['settings']['topic_direction']=='Lessons about product design'
 assert api(a,'studio')[1]['settings']['project_name']=='Synthetic project'
 assert tool('set_content_direction',{'text':'x'*1001}).get('isError')
 assert api(b,'studio')[1]['settings']['topic_direction']==''
 ok('MCP updates editorial direction in the app without changing other preferences')
 drafts=[{'evidence_id':eid,'brief_id':bid,'channel':'x','body':'A synthetic test opening.','details':'1. Synthetic problem.\n2. Synthetic decision.\n3. Next step.','visual':None}]
 batch=str(uuid.uuid4())
 assert not tool('save_content_drafts',{'drafts':drafts,'batch_key':batch}).get('isError')
 assert not tool('save_content_drafts',{'drafts':drafts,'batch_key':batch}).get('isError')
 state=api(a,'studio')[1];assert len(state['drafts'])==1
 draft=state['drafts'][0];did=draft['id'];assert draft['details']==drafts[0]['details']
 assert tool('save_content_drafts',{'drafts':[{**drafts[0],'channel':'instagram'}],'batch_key':str(uuid.uuid4())}).get('isError')
 ok('MCP round-trip preserves full thread, brief link and idempotence')
 value={'body':draft['body'],'details':'Only the details changed.','visual':None,'provider_channel_id':None,'scheduled_at':None}
 assert api(a,'edit',id=did,revision=1,draft=value)[0]==200
 draft=api(a,'studio')[1]['drafts'][0]
 assert draft['revision']==2 and draft['state']=='draft' and draft['details']==value['details']
 assert api(a,'edit',id=did,revision=1,draft=value)[0]==409
 assert api(b,'edit',id=did,revision=2,draft=value)[0]==404
 assert call('/rest/v1/bip_drafts?select=id',token=b)[1]==[]
 ok('Details-only edits increment revision; stale and cross-owner changes rejected')
 for action in ['send','approve','connect','disconnect']:
  assert api(a,action,id=did,revision=2)[0]==400
 assert tool('publish').get('isError')
 ok('Publishing and provider connection endpoints are unavailable')
 assert api(a,'settings',expected_owner=users[1][0],settings={})[0]==409
 ok('Account-switch race cannot mutate a different studio')
 assert api(a,'evidence',id=eid,patch={'state':'dismissed'})[0]==200
 hidden=context();assert not hidden['evidence'] and not hidden['briefs'] and not hidden['drafts']
 assert tool('save_content_drafts',{'drafts':drafts,'batch_key':str(uuid.uuid4())}).get('isError')
 ok('Unselecting evidence withdraws its context, briefs and drafts from MCP')
 tids=api(a,'studio')[1]['tokens'];assert api(a,'revoke_token',id=tids[0]['id'])[0]==200
 assert mcp('tools/list')[0]==401
 ok('Revocation immediately invalidates assistant access')
 if os.environ.get('FLOWSIGHT_BIP_RESULT'):pathlib.Path(os.environ['FLOWSIGHT_BIP_RESULT']).write_text(json.dumps({'passed':results,'count':len(results)},indent=2))
 print(json.dumps({'count':len(results),'passed':True}))
finally:
 for uid,_ in users:
  status,_=call('/auth/v1/admin/users/'+uid,method='DELETE');print('Removed synthetic test user:',status==200)
