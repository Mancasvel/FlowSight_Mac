// Actual production UI with fictional Google events and synthetic native IPC.
// No real account, calendar token, activity, or provider request is used.
import assert from 'node:assert/strict';
import {mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {chromium} from 'playwright';

const output=resolve('.impeccable/review/linked-session'); await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true}); const evidence=[];
try {
 for(const locale of ['es-ES','en-GB']) {
  const context=await browser.newContext({locale,timezoneId:'Europe/Madrid',viewport:{width:370,height:700},colorScheme:'dark',reducedMotion:'reduce'});
  const page=await context.newPage(); const errors=[]; page.on('pageerror',error=>errors.push(error.message));
  await page.clock.setFixedTime(new Date('2026-10-01T10:00:00+02:00'));
  await page.addInitScript(({locale})=>{
   window.testCalls=[]; let callbacks=new Map(),counter=1;
   const target={provider:'google',ownerUserId:'example',calendarId:'example@example.invalid'};
   const blocks=Array.from({length:7},(_,index)=>({id:`block-${index}`,title:index%2 ? (locale==='es-ES'?'Descanso':'Break'):`Ejercicio ${index/2+1} de ADDA`,startAt:new Date(Date.parse('2026-10-01T11:35:00+02:00')+index*25*60000).toISOString(),endAt:new Date(Date.parse('2026-10-01T11:35:00+02:00')+(index*25+ (index%2?10:25))*60000).toISOString(),provider:'google',externalId:null}));
   window.testData={events:[],sessionSaves:[]}; window.testRecovered=false;
   const responses={initialize_agent:null,get_config:{captureInterval:60000,dailyGoalHours:6},get_auth_session:null,get_current_user:{user_id:'example',email:'example@example.invalid'},get_entitlements:{plan:'individual',status:'active',can_integrations:true,can_cloud_ai:false,team_ids:[]},get_privacy_settings:{monitoringNoticeAcknowledged:true,cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30},get_user_preferences:{onboardingCompleted:true,dailyGoalHours:6,workRoles:[],workActivities:[],improvementGoals:[]},get_analytics_consent:{decided:true,consented:false},get_status:{isRunning:false},check_installation_health:{healthy:true},check_local_server:{online:false},get_week_summary:{days:[]},get_today_history:{date:'2026-10-01',total_seconds:0,entries:[],category_breakdown:[],ticket_breakdown:[],focus:{}},get_calendar_companion_status:{googleConnected:true,microsoftConnected:false,googleAvailable:true,current:null},get_desktop_preferences:{focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true},get_language_settings:{preference:'system',systemLanguage:locale==='es-ES'?'es':'en'},get_coach_chat_messages:[],get_coach_chat_usage:{used:0}};
   window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
   window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},transformCallback(fn){const id=counter++;callbacks.set(id,fn);return id;},unregisterCallback(id){callbacks.delete(id);},convertFileSrc(value){return value;},async invoke(command,args={}){
    window.testCalls.push({command,args});
    if(command==='get_local_agent_data')return structuredClone(window.testData);
    if(command==='propose_session_plan')return {id:'reviewed-four',summary:'4 ejercicios, con descansos.',calendarDestination:target,expiresInSeconds:1800,blocks,unscheduled:[]};
    if(command==='confirm_session_plan'){
     if(!window.testRecovered){window.testData.sessionSaves=[{id:args.id,target,events:blocks.map((event,index)=>({...event,externalId:index<2?`remote-${index}`:null})),complete:false}];throw new Error('Synthetic network interruption');}
     window.testData.events=blocks.map((event,index)=>({...event,externalId:`remote-${index}`}));window.testData.sessionSaves=[{id:args.id,target,events:window.testData.events,complete:true}];return {events:window.testData.events,calendarDestination:target};
    }
    if(command==='abandon_session_plan'){
     const save=window.testData.sessionSaves.find(save=>save.id===args.id);save.abandoned=true;
     window.testData.events=save.events.filter(event=>event.externalId);
     return {confirmedBlocks:window.testData.events.length,uncertainBlocks:save.events.length-window.testData.events.length,calendarDestination:target};
    }
    if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
    if(command==='plugin:event|listen')return args.handler;
    if(command.startsWith('plugin:'))return null;
    return responses[command]??null;
   }};
  },{locale});
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1450',{waitUntil:'networkidle'});
  await page.locator('#sessionPlannerToggle').click();
  await page.locator('#sessionIntention').fill('Quiero hacer 4 ejercicios de ADDA'); await page.locator('#sessionStart').fill('11:35'); await page.locator('#sessionEnd').fill('16:35');
  await page.locator('#sessionGenerate').click(); await page.locator('#sessionProposal').waitFor({state:'visible'});
  assert.match(await page.locator('#sessionPlanStatus').innerText(),/Google Calendar/);
  assert.equal(await page.locator('#sessionPlanBlocks li').count(),7);
  assert.equal(await page.evaluate(()=>window.testCalls.filter(call=>call.command==='confirm_session_plan').length),0);
  await page.locator('#sessionConfirm').click(); await page.locator('#sessionRecovery').waitFor({state:'visible'});
  assert.match(await page.locator('#sessionRecoveryStatus').innerText(),locale==='es-ES'?/2 de 7/:/2 of 7/);
  assert.equal(await page.locator('#sessionGenerate').isDisabled(),true); assert.equal(await page.locator('#sessionRetrySave').isEnabled(),true);
  assert.equal(await page.locator('#sessionCalendar').isVisible(),false);
  await page.locator('#sessionRecovery').scrollIntoViewIfNeeded(); await page.screenshot({path:resolve(output,`partial-${locale}.png`)});
  // Simulate closing/reopening app: persisted journal is displayed, no autosave.
  const saved=await page.evaluate(()=>window.testData);
  page.once('dialog',dialog=>dialog.dismiss());await page.locator('#sessionAbandonSave').click();
  assert.equal(await page.evaluate(()=>window.testCalls.filter(call=>call.command==='abandon_session_plan').length),0);
  page.once('dialog',dialog=>{assert.match(dialog.message(),locale==='es-ES'?/se conservarán/:/will stay/);return dialog.accept();});
  await page.locator('#sessionAbandonSave').click();await page.locator('#sessionRecovery').waitFor({state:'hidden'});
  assert.equal(await page.locator('#sessionCalendarBlocks li').count(),2);
  assert.equal(await page.locator('#sessionGenerate').isEnabled(),true);
  await page.addInitScript(data=>{window.testData=data;window.testRecovered=true;},{...saved});
  await page.reload({waitUntil:'networkidle'}); await page.locator('#sessionRecovery').waitFor({state:'visible'});
  assert.equal(await page.evaluate(()=>window.testCalls.filter(call=>call.command==='confirm_session_plan').length),0);
  await page.locator('#sessionRetrySave').click(); await page.locator('#sessionRecovery').waitFor({state:'hidden'});
  await page.locator('#sessionPlannerToggle').click();
  await page.locator('#sessionPlanStatus').filter({hasText:locale==='es-ES'?'7 bloques añadidos a Google Calendar.':'7 blocks added to Google Calendar.'}).waitFor();
  assert.equal(await page.locator('#sessionCalendarBlocks li').count(),7);
  assert.equal(await page.locator('#sessionGenerate').isEnabled(),true);
  const calls=await page.evaluate(()=>window.testCalls);assert.deepEqual(calls.filter(call=>call.command==='confirm_session_plan').map(call=>call.args.id),['reviewed-four']);
  assert.equal(calls.some(call=>['start_monitoring','stop_monitoring','delete_all_local_data','connect_calendar'].includes(call.command)),false);
  await page.locator('#sessionCalendar').scrollIntoViewIfNeeded();await page.screenshot({path:resolve(output,`saved-${locale}.png`)});
  assert.deepEqual(errors,[]); evidence.push({locale,passed:true,blocks:7,workBlocks:4,breaks:3,syntheticNativeIpc:true,realCalendarUntouched:true}); await context.close();
 }
 await writeFile(resolve(output,'verification.json'),JSON.stringify(evidence,null,2));console.log(JSON.stringify(evidence));
} finally {await browser.close();}
