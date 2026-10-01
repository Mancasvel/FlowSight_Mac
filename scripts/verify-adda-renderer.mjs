// Actual renderer replaying the measured local Qwen response. No native process,
// personal database, tracking, or calendar is used. Screenshots are UI previews.
import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright';

const output=resolve('.impeccable/review');
await mkdir(output,{recursive:true});
const evidence=JSON.parse(await readFile(new URL('./fixtures/adda-qwen-session.json',import.meta.url),'utf8'));
const browser=await chromium.launch({headless:true});
try {
 for(const viewport of [{width:370,height:700},{width:340,height:400}]) {
  const page=await browser.newPage({viewport,timezoneId:'Europe/Madrid',locale:'en-GB',colorScheme:'dark',reducedMotion:'reduce'});
  await page.clock.setFixedTime(new Date('2026-10-01T08:00:00+02:00'));
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript(({first,revised})=>{
   window.testCalls=[];window.testOverflow=false;
   const defaults={
    initialize_agent:null,get_config:{captureInterval:60000,dailyGoalHours:6},get_auth_session:null,get_current_user:null,
    get_entitlements:{plan:'free',status:'active',can_integrations:false,can_cloud_ai:false,can_sync:false,team_ids:[]},
    get_privacy_settings:{monitoringNoticeAcknowledged:false,cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30},
    get_analytics_consent:{decided:true,consented:false},get_status:{isRunning:false},check_installation_health:{healthy:true},check_local_server:{online:false},
    get_week_summary:{days:[]},get_today_history:{total_seconds:0,entries:[],category_breakdown:[],ticket_breakdown:[],focus:{}},
    get_calendar_companion_status:{googleConnected:false,microsoftConnected:false,googleAvailable:false,microsoftAvailable:false,current:null},
    get_notion_status:{connected:false},get_coach_chat_messages:[],get_coach_chat_usage:{used:0},get_browser_pairing:{connected:false},
    get_local_agent_data:{events:[],preferences:{},tasks:[]},get_desktop_preferences:{focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true},
    get_user_preferences:{onboardingCompleted:true,displayName:'',workRoles:[],workActivities:[],improvementGoals:[],dailyGoalHours:6},
    get_weekly_report_schedule:{enabled:false,weekday:5,time:'17:00',folder:'',revision:0},
   };
   let counter=1;window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
   window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
    transformCallback(){return counter++;},unregisterCallback(){},convertFileSrc(path){return path;},
    async invoke(command,args={}){
     window.testCalls.push({command,args});
     if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
     if(command==='plugin:event|listen')return args.handler;
     if(command.startsWith('plugin:event|')||command.startsWith('plugin:updater|'))return null;
     if(command==='plugin:app|version')return '5.0.10';
     if(command==='propose_session_plan') {
      const proposal=structuredClone(args.feedback?revised:first);
      if(window.testOverflow) proposal.unscheduled=['Genetic algorithms: 40 estimated minutes still need time after allowing for breaks and fixed commitments.','Recursive types: 75 estimated minutes still need time after allowing for breaks and fixed commitments.','PLE: 75 estimated minutes still need time after allowing for breaks and fixed commitments.'];
      return proposal;
     }
     if(command==='confirm_session_plan')throw new Error('Preview cannot save calendar events.');
     return command in defaults?structuredClone(defaults[command]):null;
    }
   };
  },{first:evidence.first.proposal,revised:evidence.revised.proposal});
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1435',{waitUntil:'networkidle'});
  await page.evaluate(()=>document.fonts.ready);
  await page.locator('#sessionPlannerToggle').click();
  await page.locator('#sessionIntention').fill('I want to do 4 exercises of ADDA related with virtual graphs, genetic algorithms, recursive types and PLE');
  await page.locator('#sessionStart').fill('10:35');await page.locator('#sessionEnd').fill('16:35');
  await page.locator('#sessionGenerate').click();
  await page.locator('#sessionProposal').waitFor({state:'visible'});
  assert.equal(await page.locator('#sessionPlanBlocks li').count(),7);
  assert.equal(await page.locator('#sessionPlanBlocks li strong').filter({hasText:'Break'}).count(),3);
  await page.locator('#sessionProposal').evaluate(element=>element.scrollIntoView({block:'start'}));
  await page.screenshot({path:resolve(output,`adda-qwen-replay-${viewport.width}x${viewport.height}.png`)});
  await page.locator('#sessionConfirm').scrollIntoViewIfNeeded();
  assert.equal(await page.locator('#sessionConfirm').isVisible(),true);
  await page.screenshot({path:resolve(output,`adda-qwen-replay-actions-${viewport.width}x${viewport.height}.png`)});
  await page.locator('#sessionFeedback').fill('Do PLE first and leave a 15-minute break between tasks.');
  await page.locator('#sessionRevise').click();
  await page.locator('#sessionPlanSummary').filter({hasText:'15-minute breaks'}).waitFor();
  assert.equal(await page.locator('#sessionPlanBlocks li strong').first().textContent(),'PLE');
  await page.locator('#sessionProposal').evaluate(element=>element.scrollIntoView({block:'start'}));
  await page.screenshot({path:resolve(output,`adda-revised-qwen-replay-${viewport.width}x${viewport.height}.png`)});
  await page.locator('#sessionConfirm').scrollIntoViewIfNeeded();
  await page.screenshot({path:resolve(output,`adda-revised-qwen-replay-actions-${viewport.width}x${viewport.height}.png`)});
  await page.evaluate(()=>{window.testOverflow=true;});
  await page.locator('#sessionGenerate').click();
  await page.locator('#sessionUnscheduled').waitFor({state:'visible'});
  await page.locator('#sessionUnscheduled').scrollIntoViewIfNeeded();
  await page.screenshot({path:resolve(output,`adda-overflow-layout-preview-${viewport.width}x${viewport.height}.png`)});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  assert.equal(await page.evaluate(()=>window.testCalls.some(call=>['confirm_session_plan','start_monitoring'].includes(call.command))),false);
  assert.deepEqual(errors,[]);
  console.log(`${viewport.width}x${viewport.height}: replayed local Qwen draft/revision, 7 blocks, 3 rests, overflow copy, no horizontal overflow or native writes.`);
  await page.close();
 }
}finally{await browser.close();}
