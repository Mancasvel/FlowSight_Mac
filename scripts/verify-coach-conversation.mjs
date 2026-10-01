// Production UI with a fictional Pro account and synthetic saved conversation.
// No account, activity, token, calendar or cloud inference request is used.
import assert from 'node:assert/strict';
import {mkdir,writeFile}from'node:fs/promises';
import {resolve}from'node:path';
import {chromium}from'playwright';

const output=resolve('.impeccable/review/coach');await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});const evidence=[];
try {
 for(const locale of ['es-ES','en-GB']) {
  const context=await browser.newContext({locale,timezoneId:'Europe/Madrid',viewport:{width:370,height:700},colorScheme:'dark',reducedMotion:'reduce'});
  await context.addInitScript(({locale})=>{
   let next=1;const callbacks=new Map();window.testCalls=[];
   const messages=[
    {id:'u-saved',role:'user',content:'Review · 学習: keep my saved question.'},
    {id:'a-saved',role:'assistant',content:'# Plan\n\n**PLE first**\n\n## Next\n\n1. Work on PLE\n2. Take a 15-minute break\n\nKeep Settings · Review · 学習 unchanged.'},
   ];
   const entitlements={plan:'individual',status:'active',can_cloud_ai:true,can_integrations:false,can_sync:false,team_ids:[]};
   const privacy={monitoringNoticeAcknowledged:true,cloudAiEnabled:true,cloudSyncEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30};
   const prefs={onboardingCompleted:true,displayName:'Example',dailyGoalHours:6,workRoles:[],workActivities:[],improvementGoals:[]};
   const responses={initialize_agent:null,get_config:{captureInterval:60000,dailyGoalHours:6},get_auth_session:null,
    get_current_user:{user_id:'fictional-pro',email:'example@example.invalid'},get_entitlements:entitlements,refresh_entitlements:entitlements,
    get_privacy_settings:privacy,get_user_preferences:prefs,get_analytics_consent:{decided:true,consented:false},
    get_status:{isRunning:false},check_installation_health:{healthy:true},check_local_server:{online:false},get_week_summary:{days:[]},
    get_today_history:{date:'2026-10-01',total_seconds:0,entries:[],category_breakdown:[],ticket_breakdown:[],focus:{}},
    get_calendar_companion_status:{googleConnected:false,microsoftConnected:false,googleAvailable:false,microsoftAvailable:false,current:null},
    get_desktop_preferences:{focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true},
    get_weekly_report_schedule:{enabled:false,weekday:5,time:'17:00',folder:'',revision:0},get_browser_pairing:{connected:false},
    get_local_agent_data:{events:[],preferences:{},tasks:[]},get_user_teams:[],get_coach_chat_usage:{usage:{used:1,limit:150,remaining:149}}};
   window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
   window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
    transformCallback(fn){const id=next++;callbacks.set(id,fn);return id;},unregisterCallback(id){callbacks.delete(id);},convertFileSrc(p){return p;},
    async invoke(command,args={}){
     window.testCalls.push({command,args});
     if(command==='get_language_preference')return{preference:'system',language:locale.startsWith('es')?'es':'en',systemLanguage:locale.startsWith('es')?'es':'en',persisted:true};
     if(command==='set_language_preference')return{preference:args.preference,language:args.preference,systemLanguage:locale.startsWith('es')?'es':'en',persisted:true};
     if(command==='get_coach_chat_messages')return structuredClone(messages);
     if(command==='send_coach_chat_message'){
      messages.push({id:'u-new',role:'user',content:args.message},{id:'a-new',role:'assistant',content:'A plain reply: Review · 学習.\n\n<script>alert("bad")</script>\n\n- Take a 15-minute break'});
      return{messages:structuredClone(messages),reply:messages.at(-1).content,usage:{used:2,limit:150,remaining:148}};
     }
     if(command==='plugin:app|version')return'5.0.12';
     if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
     if(command.startsWith('plugin:'))return null;
     return command in responses?structuredClone(responses[command]):null;
    }};
  },{locale});
  const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
  const warnings=[];page.on('console',msg=>{if(msg.type()==='warning'&&msg.text().includes('[Coach] conversation unavailable'))warnings.push(msg.text());});
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1450',{waitUntil:'networkidle'});
  await page.locator('#navCloudInsights').click();
  await page.locator('#coachMessages .coach-msg.assistant h1').waitFor();
  assert.equal(await page.locator('#coachLockOverlay').isVisible(),false);
  assert.equal(await page.locator('#coachMessages h1').textContent(),'Plan');
  assert.equal(await page.locator('#coachMessages li').count(),2);
  assert.match(await page.locator('#coachMessages').innerText(),/Review · 学習/);
  assert.match(await page.locator('#coachUsageBar').innerText(),/1\/150/);
  await page.screenshot({path:resolve(output,`saved-conversation-${locale}.png`)});
  await page.locator('#coachInput').fill('Keep my existing conversation and add a break.');
  await page.locator('#coachSendBtn').click();
  await page.locator('#coachMessages .coach-msg.assistant').nth(1).waitFor();
  assert.equal(await page.locator('#coachMessages .coach-msg').count(),4);
  assert.equal(await page.locator('#coachMessages script').count(),0);
  assert.match(await page.locator('#coachMessages').innerText(),/<script>alert\("bad"\)<\/script>/);
  assert.equal(await page.locator('#coachInput').inputValue(),'');
  assert.equal(await page.locator('#coachSendBtn').isEnabled(),true);
  await page.locator('#navProfile').click();await page.locator('#navCloudInsights').click();
  assert.equal(await page.locator('#coachMessages .coach-msg').count(),4);
  assert.deepEqual(errors,[]);assert.deepEqual(warnings,[]);
  assert.doesNotMatch(await page.locator('body').innerText(),/No se pudo cargar la conversación con Coach|Could not load your Coach conversation/);
  const calls=await page.evaluate(()=>window.testCalls);
  assert.equal(calls.filter(call=>call.command==='send_coach_chat_message').length,1);
  assert.equal(calls.filter(call=>['delete_all_local_data','start_monitoring','stop_monitoring','confirm_session_plan'].includes(call.command)).length,0);
  evidence.push({locale,savedHistoryLoaded:true,markdownRendered:true,newReplyRendered:true,reopenHistoryPreserved:true,scriptEscaped:true,errors,warnings,syntheticNative:true});
  console.log(`${locale}: saved Pro conversation, reply, Markdown, reopen and escaping passed.`);await context.close();
 }
 await writeFile(resolve(output,'verification.json'),JSON.stringify(evidence,null,2));
}finally{await browser.close();}
