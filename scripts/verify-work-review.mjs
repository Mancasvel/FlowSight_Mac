// Actual compiled renderer; synthetic native storage, no personal data or cloud calls.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { mkdir } from 'node:fs/promises';
const folder=process.env.FLOWSIGHT_REVIEW_OUTPUT||'.impeccable/review';
await mkdir(folder,{recursive:true});
const browser=await chromium.launch({headless:true});
async function openSaved(page) {
 await page.locator('[data-review-edit]').first().waitFor({state:'attached'});
 await page.locator('.work-review-history').evaluate(node=>{node.open=true;});
}
try {
 for(const view of [{width:370,height:700,name:'review-compact',language:'en',dark:false},{width:340,height:400,name:'review-small-es',language:'es',dark:false},{width:900,height:800,name:'review-wide-dark',language:'en',dark:true}]) {
  const context=await browser.newContext({viewport:view,locale:view.language==='es'?'es-ES':'en-GB',colorScheme:view.dark?'dark':'light',reducedMotion:'reduce'});
  await context.route(/https?:\/\/(?!127\.0\.0\.1)/,route=>route.abort());
  await context.addInitScript(({language})=>{
    localStorage.setItem('flowsight_language_preference',language);
    window.reviewCalls=[];window.reviewFailSave=false;window.reviewFailDelete=false;window.reviewEmpty=false;
    const callbacks=new Map();let counter=1;
    const prefs=JSON.parse(localStorage.getItem('test-review-prefs')||'null')||{onboardingCompleted:true,displayName:'Review · María',workRoles:[],workActivities:[],improvementGoals:[],dailyGoalHours:0};
    const privacy={monitoringNoticeAcknowledged:true,noticeVersion:'2026-09-28',cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30};
    const data={date:'2026-10-10',total_seconds:1800,entries:[{time:'2026-10-10T09:00:00',duration_seconds:1800,category:'Writing',description:'Synthetic writing'}],category_breakdown:[{category:'Writing',total_seconds:1800}],ticket_breakdown:[],focus:{}};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
    window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},transformCallback(fn){const id=counter++;callbacks.set(id,fn);return id;},unregisterCallback(id){callbacks.delete(id);},convertFileSrc(path){return path;},async invoke(command,args={}){
      window.reviewCalls.push({command,args});
      if(command==='plugin:event|listen')return args.handler;
      if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
      if(command.startsWith('plugin:event|')||command.startsWith('plugin:updater|'))return null;
      if(command==='plugin:app|version')return '6.1.0';
      if(command==='get_work_review_decisions')return JSON.parse(localStorage.getItem('test-review-decisions')||'[]');
      if(command==='save_work_review_decision'){
        if(window.reviewFailSave)throw Error('Synthetic disk failure');
        const list=JSON.parse(localStorage.getItem('test-review-decisions')||'[]');
        if(args.input.id&&!list.some(item=>item.id===args.input.id))throw Error('This decision is no longer available. Refresh the review.');
        const saved={...args.input,id:args.input.id||'synthetic-decision',createdAt:'2026-10-10',updatedAt:'2026-10-10'};
        localStorage.setItem('test-review-decisions',JSON.stringify([saved,...list.filter(item=>item.id!==saved.id)]));return saved;
      }
      if(command==='delete_work_review_decision'){if(window.reviewFailDelete)throw Error('Synthetic disk failure');const list=JSON.parse(localStorage.getItem('test-review-decisions')||'[]');localStorage.setItem('test-review-decisions',JSON.stringify(list.filter(item=>item.id!==args.id)));return null;}
      if(command==='save_user_preferences_command'){localStorage.setItem('test-review-prefs',JSON.stringify(args.prefs));return args.prefs;}
      if(command==='get_user_preferences')return prefs;
      if(command==='get_privacy_settings')return privacy;
      if(command==='get_today_history')return window.reviewEmpty?{...data,total_seconds:0,entries:[],category_breakdown:[]}:data;
      if(command==='generate_local_status_report'){
        const empty=window.reviewEmpty,days=args.periodDays;
        return {ai_powered:false,generated_at:'2026-10-10 10:00',local_data:{period_start:days===1?'2026-10-10':days===7?'2026-10-04':'2026-09-11',period_end:'2026-10-10',period_days:days,total_seconds:empty?0:1800,active_days:empty?0:1,category_breakdown:empty?[]:data.category_breakdown},report:{executive_overview:empty?'No activity was recorded.':'A writing block was observed.',recommendations:empty?[]:['Reserve a writing block.'],overall_health:'Observed'}};
      }
      if(command==='get_local_agent_data')return {events:[],preferences:{},tasks:[]};
      if(command==='get_total_focus')return {preferences:{patterns:[],exceptions:[],durationMinutes:50,quietNotifications:false},session:null,browser:{},digest:[]};
      const values={initialize_agent:null,get_config:{dailyGoalHours:0,captureInterval:60000},get_auth_session:null,get_current_user:null,get_entitlements:{plan:'free',status:'active',can_sync:false,can_cloud_ai:false,can_integrations:false,team_ids:[]},get_analytics_consent:{decided:true,consented:false},get_status:{isRunning:false},get_tracking_clock:{date:'2026-10-10',total_seconds:1800,total_milliseconds:1800000,is_running:false},check_installation_health:{healthy:true},check_local_server:{online:false},get_week_summary:{days:[]},get_desktop_preferences:{promptDecided:true,focusAlertsEnabled:false,contextualFocusAlertsEnabled:false},get_weekly_report_schedule:{enabled:false,weekday:5,time:'17:00',folder:'',revision:0},get_calendar_companion_status:{},get_browser_pairing:{},get_notion_status:{connected:false},get_coach_chat_messages:[]};
      return values[command]??null;
    }};
  },{language:view.language});
  const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1443',{waitUntil:'networkidle'});
  await page.locator('.nav-item[data-tab="tabSummary"]').click();
  await page.locator('#workReportPeriod').waitFor();
  assert.equal(await page.locator('#workReportPeriod').inputValue(),'7');
  await page.locator('#workReportPeriod').selectOption('1');
  await page.locator('#generateReportBtn').click();
  await page.locator('[data-review-suggestion="0"]').waitFor();
  assert.match(await page.locator('.sr-origin').first().innerText(),view.language==='es'?/Informe basado en reglas/:/Rule-based report/);
  await page.evaluate(()=>document.fonts.ready);
  await page.locator('.sr-origin').first().scrollIntoViewIfNeeded();
  await page.screenshot({path:`${folder}/${view.name}-origin.png`});
  await page.locator('[data-review-suggestion="0"]').click();
  await page.locator('#reviewDecisionText').fill('Review · 東京 <script>not executable</script>');
  await page.locator('#reviewDecisionDate').fill('2026-10-17');
  page.once('dialog',dialog=>dialog.dismiss());await page.locator('[data-review-suggestion="0"]').click();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Review · 東京 <script>not executable</script>');
  page.once('dialog',dialog=>dialog.accept());await page.locator('[data-review-suggestion="0"]').click();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Reserve a writing block.');
  await page.locator('#reviewDecisionText').fill('Review · 東京 <script>not executable</script>');await page.locator('#reviewDecisionDate').fill('2026-10-17');
  await page.evaluate(()=>window.reviewFailSave=true);
  await page.locator('#reviewDecisionForm button[type="submit"]').click();
  await page.locator('#reviewDecisionMessage').filter({hasText:view.language==='es'?'borrador':'draft'}).waitFor();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Review · 東京 <script>not executable</script>');
  // Language switch must retain the editable draft and date.
  await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value=select.value==='es'?'en':'es';select.dispatchEvent(new Event('change',{bubbles:true}));});
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Review · 東京 <script>not executable</script>');
  assert.equal(await page.locator('#reviewDecisionDate').inputValue(),'2026-10-17');
  await page.evaluate(()=>{window.reviewFailSave=false;});
  await page.locator('#reviewDecisionForm button[type="submit"]').click();
  await openSaved(page);
  await page.reload({waitUntil:'networkidle'});
  await page.locator('.nav-item[data-tab="tabSummary"]').click();await page.locator('#generateReportBtn').click();
  await openSaved(page);
  await page.locator('[data-review-edit]').click();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Review · 東京 <script>not executable</script>');
  await page.locator('#reviewDecisionStatus').selectOption('tried');await page.locator('#reviewDecisionNote').fill('Useful, keep Tuesdays free.');
  await page.locator('#reviewDecisionForm button[type="submit"]').click();await openSaved(page);await page.locator('.work-review-note').waitFor();
  await page.evaluate(language=>{const select=document.getElementById('languageSelect');select.value=language;select.dispatchEvent(new Event('change',{bubbles:true}));},view.language);
  await openSaved(page);
  await page.locator('.work-review').scrollIntoViewIfNeeded();await page.evaluate(()=>document.fonts.ready);
  await page.screenshot({path:`${folder}/${view.name}.png`});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
  await page.locator('[data-review-edit]').click();
  await page.locator('.work-review-form').scrollIntoViewIfNeeded();await page.screenshot({path:`${folder}/${view.name}-form.png`});
  await page.locator('#reviewDecisionForm button[type="submit"]').scrollIntoViewIfNeeded();await page.screenshot({path:`${folder}/${view.name}-actions.png`});
  await page.locator('#reviewDecisionText').fill('Edited draft, not saved');
  await page.locator('[data-review-edit]').click();assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Edited draft, not saved');
  await page.evaluate(()=>{localStorage.setItem('test-review-decisions','[]');});
  await page.locator('#reviewDecisionForm button[type="submit"]').click();
  await page.locator('#reviewDecisionMessage').filter({hasText:view.language==='es'?'ya no está':'no longer available'}).waitFor();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Edited draft, not saved');
  await page.locator('#reviewDecisionMessage').scrollIntoViewIfNeeded();await page.screenshot({path:`${folder}/${view.name}-recovery.png`});
  await page.locator('#reviewDecisionForm button[type="submit"]').click();await openSaved(page);await page.locator('[data-review-edit]').click();
  await page.locator('#reviewDecisionText').fill(' ');await page.locator('#reviewDecisionForm button[type="submit"]').click();
  await page.locator('#reviewDecisionMessage').filter({hasText:view.language==='es'?'Escribe':'Write a choice'}).waitFor();
  await page.locator('#reviewDecisionText').fill('Recovered choice');
  await page.evaluate(()=>window.reviewFailDelete=true);page.once('dialog',dialog=>dialog.accept());await page.locator('#reviewDeleteBtn').click();
  await page.locator('#reviewDecisionMessage').filter({hasText:view.language==='es'?'eliminar':'delete'}).waitFor();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Recovered choice');await page.evaluate(()=>window.reviewFailDelete=false);
  page.once('dialog',dialog=>dialog.accept());await page.locator('#reviewDeleteBtn').click();await page.locator('[data-review-edit]').waitFor({state:'detached'});
  await page.locator('#reviewKeepBtn').click();await page.locator('#reviewDecisionForm button[type="submit"]').click();await openSaved(page);
  assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('test-review-decisions'))[0].kind),'keep');
  await page.locator('#closeStatusReportModal').click();
  for(const days of ['7','30']) {
    await page.locator('#workReportPeriod').selectOption(days);
    await page.locator('#generateReportBtn').click();
    await page.locator('#reviewNewBtn').waitFor();
    assert.equal(await page.evaluate(()=>window.reviewCalls.filter(c=>c.command==='generate_local_status_report').at(-1).args.periodDays),Number(days));
    await page.locator('#closeStatusReportModal').click();
    assert.equal(await page.locator('#workReportPeriod').inputValue(),days);
  }
  await page.evaluate(()=>window.reviewEmpty=true);
  await page.locator('#workReportPeriod').selectOption('1');await page.locator('#generateReportBtn').click();
  await page.locator('#reviewNewBtn').waitFor();
  assert.equal(await page.locator('[data-review-suggestion]').count(),0);
  assert.match(await page.locator('.sr-origin').first().innerText(),/0/);
  await page.locator('#closeStatusReportModal').click();
  await page.evaluate(()=>{const list=JSON.parse(localStorage.getItem('test-review-decisions'));localStorage.setItem('test-review-decisions',JSON.stringify([...list,{...list[0],id:'second-choice',text:'Another saved choice'}]));});
  await page.locator('#generateReportBtn').click();await openSaved(page);await page.locator('[data-review-edit="synthetic-decision"]').click();
  await page.locator('#reviewDecisionText').fill('Unsaved history edit');page.once('dialog',dialog=>dialog.dismiss());await page.locator('[data-review-edit="second-choice"]').click();
  assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Unsaved history edit');
  page.once('dialog',dialog=>dialog.accept());await page.locator('[data-review-edit="second-choice"]').click();assert.equal(await page.locator('#reviewDecisionText').inputValue(),'Another saved choice');
  await page.locator('#reviewCancelBtn').click();await page.locator('#closeStatusReportModal').click();
  await page.locator('.nav-item[data-tab="tabProfile"]').click();await page.locator('#editWorkPreferencesBtn').click();
  await page.locator('#onboardingNameInput').fill('Local preference · María');
  await page.locator('#onboardingSkipCalendarBtn').click();await page.locator('#onboardingOverlay.visible').waitFor({state:'detached'});
  assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('test-review-prefs')).displayName),'Local preference · María');
  await page.reload({waitUntil:'networkidle'});await page.locator('.nav-item[data-tab="tabProfile"]').click();await page.locator('#editWorkPreferencesBtn').click();
  assert.equal(await page.locator('#onboardingNameInput').inputValue(),'Local preference · María');
  assert.equal(await page.evaluate(()=>window.reviewCalls.filter(call=>['sync_now','get_supabase_client','sign_in'].includes(call.command)).length),0);
  assert.equal(errors.length,0,errors.join('\n'));
  await context.close();
 }
 console.log('PASS: local report periods, save/retry, language-preserved draft, reopen/edit/delete/keep, compact/dark renderer, no cloud sync.');
} finally {await browser.close();}
