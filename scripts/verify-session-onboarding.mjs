// Browser integration checks against the actual renderer with isolated synthetic
// native responses. No user's local database, tracking, or calendars are changed.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const output = new URL(process.env.FLOWSIGHT_EVIDENCE_URL || '../.impeccable/review/', import.meta.url);
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
async function assertTimerLabelFits(page, selector) {
  const result = await page.locator(selector).evaluate(button => {
    const label=button.querySelector('span'),box=button.getBoundingClientRect();
    const range=document.createRange();range.selectNodeContents(label);
    const inside=rect=>rect.left>=box.left+1 && rect.right<=box.right-1 && rect.top>=box.top+1 && rect.bottom<=box.bottom-1;
    return {text:label.textContent, fits:[...range.getClientRects()].every(inside) && [...button.querySelectorAll('svg')].filter(icon=>getComputedStyle(icon).display!=='none').every(icon=>inside(icon.getBoundingClientRect())),
      width:box.width,height:box.height,scrollWidth:button.scrollWidth,clientWidth:button.clientWidth};
  });
  assert.equal(result.fits,true,`Timer label and icon must fit: ${JSON.stringify(result)}`);
  assert.ok(result.scrollWidth<=result.clientWidth+1,`Timer control must not overflow: ${JSON.stringify(result)}`);
}
async function inspectSpanishClock(page, viewport, paused=false) {
  await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value='es';select.dispatchEvent(new Event('change',{bubbles:true}));});
  assert.equal(await page.locator('#playTimerLabel').innerText(),paused?'Reanudar concentración total':'Pausar concentración total');
  await assertTimerLabelFits(page,'#playTimerBtn');
  const sizes=await page.locator('.timer-actions').evaluate(row=>[...row.querySelectorAll('button')].filter(button=>getComputedStyle(button).display!=='none').map(button=>button.getBoundingClientRect().height));
  assert.ok(Math.max(...sizes)-Math.min(...sizes)<1,'Pause/resume and Stop must align in height.');
  await page.locator('#playTimerBtn').scrollIntoViewIfNeeded();
  await page.screenshot({path:fileURLToPath(new URL(`total-focus-${paused?'resume':'pause'}-fit-es-${viewport.name}.png`,output))});
  // Text expansion must reflow instead of escaping the button.
  await page.locator('#playTimerBtn').evaluate(button=>button.style.fontSize='28px');
  await assertTimerLabelFits(page,'#playTimerBtn');
  await page.locator('#playTimerBtn').evaluate(button=>button.style.removeProperty('font-size'));
  await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value='en';select.dispatchEvent(new Event('change',{bubbles:true}));});
}
try {
  for (const viewport of [{width:370,height:700,name:'compact'},{width:340,height:400,name:'small'},{width:900,height:800,name:'wide-dark',dark:true}]) {
    if(process.env.FLOWSIGHT_TEST_VIEWPORT && process.env.FLOWSIGHT_TEST_VIEWPORT!==viewport.name)continue;
    const page = await browser.newPage({viewport:{width:viewport.width,height:viewport.height},timezoneId:'Europe/Madrid',locale:'en-GB',colorScheme:viewport.dark?'dark':'light',reducedMotion:'reduce'});
    await page.clock.setFixedTime(new Date('2026-10-01T08:00:00+02:00'));
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(({ pro }) => {
      const calls=[];window.testCalls=calls;window.testWindowErrors=[];
      window.addEventListener('error',event=>window.testWindowErrors.push(event.message));
      window.testFailures={};
      let events=[],proposalCount=0,running=false,trackedMs=0,trackingTick=performance.now();
      const trackingSnapshot=()=>{const tick=performance.now();if(running)trackedMs+=tick-trackingTick;trackingTick=tick;return {date:'2026-10-01',total_milliseconds:trackedMs,total_seconds:Math.floor(trackedMs/1000),is_running:running};};
      let prefs={onboardingCompleted:false,displayName:'',workRoles:[],workActivities:[],improvementGoals:[],dailyGoalHours:6};
      let privacy={noticeVersion:'2026-09-28',monitoringNoticeAcknowledged:false,cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30};
      let desktop={focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true};
      let schedule={enabled:false,weekday:5,time:'17:00',folder:'',revision:0};
      let totalFocus={preferences:{patterns:['instagram.com','tiktok.com','x.com'],exceptions:[],durationMinutes:50,quietNotifications:true},session:null,browser:{connected:false,fresh:false,applied:false},messagingAvailable:false,systemNotificationsAvailable:true,systemNotificationsQuiet:false,digest:[]};
      const responses={
        initialize_agent:null,get_config:{captureInterval:60000,dailyGoalHours:6},
        get_auth_session:null,get_current_user:null,get_entitlements:{plan:pro?'pro':'free',status:'active',can_integrations:pro,can_cloud_ai:false,can_sync:false,team_ids:[]},
        get_privacy_settings:{monitoringNoticeAcknowledged:false,cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30},
        get_analytics_consent:{decided:true,consented:false},get_status:{isRunning:false},
        check_installation_health:{healthy:true},check_local_server:{online:false},
        get_week_summary:{days:[]},get_today_history:{date:'2026-10-01',total_seconds:0,entries:[],category_breakdown:[],ticket_breakdown:[],focus:{}},
        get_calendar_companion_status:{googleConnected:false,microsoftConnected:false,googleAvailable:false,microsoftAvailable:false,current:null},
        get_notion_status:{connected:false},get_coach_chat_messages:[],get_coach_chat_usage:{used:0},get_browser_pairing:{connected:false},
      };
      const callbacks=new Map();let counter=1;
      window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
      window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
        transformCallback(fn){const id=counter++;callbacks.set(id,fn);return id;},unregisterCallback(id){callbacks.delete(id);},convertFileSrc(p){return p;},
        async invoke(command,args={}) {
          calls.push({command,args});
          if(window.testDelays?.[command])await new Promise(resolve=>setTimeout(resolve,window.testDelays[command]));
          if(window.testFailures[command]) throw new Error(window.testFailures[command]);
          if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
          if(command==='plugin:event|listen')return args.handler;
          if(command.startsWith('plugin:event|')||command.startsWith('plugin:updater|'))return null;
          if(command==='plugin:app|version')return '5.0.10';
          if(command==='get_today_history' && window.testRecordedHistory)return {
            date:'2026-10-01',total_seconds:3600,category_breakdown:[],ticket_breakdown:[],focus:{},
            entries:[{time:'2026-10-01T09:00:00+02:00',duration_seconds:3600,category:'Writing',description:'Synthetic writing session'}]
          };
          if(command==='get_local_agent_data')return {events,preferences:{},tasks:[]};
          if(command==='get_user_preferences')return prefs;
          if(command==='get_privacy_settings')return structuredClone(privacy);
          if(command==='update_privacy_settings'){privacy={...privacy,...args.patch};return structuredClone(privacy);}
          if(command==='get_status')return {isRunning:running};
          if(command==='get_tracking_clock')return trackingSnapshot();
          if(command==='get_total_focus')return structuredClone({...totalFocus,digest:totalFocus.session && !totalFocus.session.pausedAt ? [] : totalFocus.digest});
          if(command==='save_total_focus_preferences'){totalFocus.preferences=args.preferences;return args.preferences;}
          if(command==='start_total_focus'){
            trackingSnapshot();running=true;
            totalFocus.session={id:'synthetic-focus',intention:args.intention.trim() || 'General',expiresAt:'2026-10-01T09:00:00+02:00',linkedClock:true,pausedAt:null,...args.preferences};
            totalFocus.systemNotificationsQuiet=args.preferences.quietNotifications;
            totalFocus.browser={connected:true,totalFocusAvailable:true,fresh:true,applied:true,sessionId:'synthetic-focus'};return structuredClone(totalFocus);
          }
          if(command==='pause_total_focus'){trackingSnapshot();running=false;totalFocus.session.pausedAt=new Date().toISOString();totalFocus.session.remainingSeconds=2900;totalFocus.browser.applied=false;totalFocus.systemNotificationsQuiet=false;return {browserReleased:true};}
          if(command==='resume_total_focus'){trackingSnapshot();running=true;totalFocus.session.pausedAt=null;totalFocus.session.expiresAt=new Date(Date.now()+2900000).toISOString();totalFocus.browser.applied=true;totalFocus.systemNotificationsQuiet=true;return structuredClone(totalFocus);}
          if(command==='end_total_focus'){trackingSnapshot();running=false;totalFocus.session=null;totalFocus.browser.applied=false;totalFocus.systemNotificationsQuiet=false;totalFocus.digest=[{id:'reminder-1',title:'Return to your task',body:'Saved during focus'}];return {browserReleased:true};}
          if(command==='dismiss_total_focus_digest'){totalFocus.digest=totalFocus.digest.filter(item=>!args.ids.includes(item.id));return null;}
          if(command==='test_focus_connection'){totalFocus.browser.connected=args.connected;totalFocus.browser.totalFocusAvailable=args.available ?? false;return null;}
          if(command==='save_user_preferences_command'){prefs=args.prefs;return prefs;}
          if(command==='get_desktop_preferences')return desktop;
          if(command==='set_focus_alerts_enabled'){desktop.focusAlertsEnabled=args.enabled;return args.enabled;}
          if(command==='set_contextual_focus_alerts_enabled'){desktop.contextualFocusAlertsEnabled=args.enabled;return args.enabled;}
          if(command==='get_weekly_report_schedule')return schedule;
          if(command==='save_weekly_report_schedule'){schedule={...args.schedule,revision:1};return schedule;}
          if(command==='plugin:dialog|open')return 'C:\\Example reports';
          if(command==='propose_session_plan') {
            proposalCount++; const start=Date.parse(args.request.startAt);
            return {id:`draft-${proposalCount}`,summary:args.feedback?'Review first, as requested.':'Allow 60 minutes for writing and 40 for review.',expiresInSeconds:1800,unscheduled:[],blocks:[
              {title:args.feedback?'Review proposal':'Write proposal',startAt:new Date(start).toISOString(),endAt:new Date(start+3600000).toISOString(),rationale:'Your stated estimate.'},
              {title:args.feedback?'Write proposal':'Review proposal',startAt:new Date(start+4200000).toISOString(),endAt:new Date(start+6600000).toISOString(),rationale:'Estimated; allow ten minutes for a break.'}
            ]};
          }
          if(command==='confirm_session_plan'){events=[{title:'Review proposal',startAt:'2026-10-01T09:00:00+02:00',endAt:'2026-10-01T10:00:00+02:00'},{title:'Write proposal',startAt:'2026-10-01T10:10:00+02:00',endAt:'2026-10-01T10:50:00+02:00'}];return {events,calendarDestination:null};}
          return command in responses?structuredClone(responses[command]):null;
        }
      };
    }, { pro:Boolean(viewport.dark) });
    await page.goto(process.env.FLOWSIGHT_RENDERER_URL || 'http://127.0.0.1:1420',{waitUntil:'networkidle'});
    await page.locator('#chooseIndividualUsage').click();
    await page.locator('#onboardingOverlay.visible').waitFor();
    await page.evaluate(()=>document.fonts.ready);
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-${viewport.name}.png`,output))});
    assert.equal(await page.locator('#onboardingContinueBtn').isEnabled(),true);
    if(viewport.name==='small') {
      await page.setViewportSize({width:342,height:402});await page.setViewportSize({width:340,height:400});
      await page.locator('#onboardingBody').evaluate(element=>{element.scrollTop=element.scrollHeight;});
      await page.locator('#onboardingMoreSettings').filter({hasText:'Back to top'}).waitFor();
      await page.locator('#onboardingMoreSettings').click();
      await page.locator('#onboardingMoreSettings').filter({hasText:'More settings below'}).waitFor();
    }
    await page.locator('#onboardingSkipCalendarBtn').focus(); await page.keyboard.press('Tab');
    assert.equal(await page.evaluate(()=>document.activeElement.id),'onboardingNameInput');
    await page.keyboard.press('Shift+Tab');
    assert.equal(await page.evaluate(()=>document.activeElement.id),'onboardingSkipCalendarBtn');
    await page.evaluate(()=>{window.testFailures.save_user_preferences_command='Storage unavailable';});
    await page.locator('#onboardingSkipCalendarBtn').click();
    await page.locator('#onboardingSetupStatus').filter({hasText:'Storage unavailable'}).waitFor();
    assert.equal(await page.locator('#onboardingSetupStatus').isVisible(),true);
    await page.evaluate(()=>{window.testFailures={};});
    if(viewport.name==='small') {
      await page.locator('#onboardingMoreSettings').filter({hasText:'More settings below'}).waitFor();
      await page.locator('#onboardingNameInput').focus();
      await page.screenshot({path:fileURLToPath(new URL('onboarding-small-focus.png',output))});
    }
    await page.locator('#onboardingContinueBtn').click();
    await page.getByRole('heading',{name:'A plan you can change'}).waitFor();
    const planLabelsFit = await page.locator('.onboarding-demo svg').evaluate(svg => {
      const blocks=[...svg.querySelectorAll('.demo-block')];
      return [...svg.querySelectorAll('.demo-labels text')].every((text,index) => {
        const label=text.getBBox(),block=blocks[index].getBBox();
        return label.x>=block.x && label.x+label.width<=block.x+block.width
          && label.y>=block.y && label.y+label.height<=block.y+block.height;
      });
    });
    assert.equal(planLabelsFit,true,'Every plan label must fit inside its own block.');
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-plan-${viewport.name}.png`,output))});
    await page.locator('#onboardingOpenPlan').check();
    await page.locator('#onboardingContinueBtn').click();
    assert.equal(await page.locator('#onboardingFocusReminders').isChecked(),false);
    assert.equal(await page.locator('#onboardingContextReminders').isDisabled(),true);
    const preview=page.locator('#onboardingNotificationPreview');
    await preview.waitFor();
    assert.match(await preview.innerText(),/Example desktop notification/);
    assert.doesNotMatch(await preview.innerText(),/Write proposal/);
    const callsBeforePreview=await page.evaluate(()=>window.testCalls.length);
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-reminder-${viewport.name}.png`,output))});
    await page.locator('#onboardingFocusReminders').check();
    assert.doesNotMatch(await preview.innerText(),/Write proposal/);
    await page.locator('#onboardingContextReminders').check();
    assert.match(await preview.innerText(),/Choose “Write proposal”/);
    assert.match(await preview.innerText(),/fictional task/);
    await preview.scrollIntoViewIfNeeded();
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-reminder-context-${viewport.name}.png`,output))});
    await page.locator('#onboardingFocusReminders').uncheck();
    assert.equal(await page.locator('#onboardingContextReminders').isDisabled(),true);
    assert.equal(await page.locator('#onboardingContextReminders').isChecked(),false);
    assert.doesNotMatch(await preview.innerText(),/Write proposal/);
    const previewCalls=await page.evaluate(index=>window.testCalls.slice(index),callsBeforePreview);
    assert.equal(previewCalls.some(c=>/notification|focus_alert|monitoring/.test(c.command)),false,'Notification example must not change native consent, tracking or send a notification.');
    await page.locator('#onboardingFocusReminders').check();
    await page.locator('#onboardingContinueBtn').click();
    await page.getByRole('heading',{name:'Make space for total focus'}).waitFor();
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-total-focus-intro-${viewport.name}.png`,output))});
    assert.match(await page.locator('#onboardingBody').innerText(),/Windows, macOS, and Linux/);
    assert.match(await page.locator('#onboardingBody').innerText(),/Coming later/);
    await page.locator('#onboardingFocusSites').fill('youtube.com\ninstagram.com');
    await page.locator('#onboardingFocusExceptions').fill('youtube.com/watch');
    await page.locator('#onboardingFocusMinutes').fill('50');
    assert.equal(await page.locator('#onboardingFocusQuiet').isChecked(),true);
    await page.locator('#onboardingFocusQuiet').uncheck();
    const musicUrl='https://music.youtube.com/playlist?list=PLfocus&index=2';
    assert.equal(await page.locator('#onboardingFocusMusicEnabled').isChecked(),false);
    await page.locator('#onboardingFocusMusicEnabled').check();
    await page.locator('#onboardingFocusMusicUrl').fill(musicUrl);
    await page.locator('#onboardingBody').evaluate(element=>{element.scrollTop=0;});
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-total-focus-${viewport.name}.png`,output))});
    await page.locator('#onboardingBody .total-focus-future').scrollIntoViewIfNeeded();
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-total-focus-future-${viewport.name}.png`,output))});
    await page.evaluate(()=>{window.testFailures.save_total_focus_preferences='Storage unavailable';});
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingSetupStatus').filter({hasText:'Storage unavailable'}).waitFor();
    assert.equal(await page.locator('#onboardingFocusSites').isEnabled(),true);
    await page.evaluate(()=>{window.testFailures={};});
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingWeeklyEnabled').check();
    assert.equal(await page.locator('#onboardingContinueBtn').isDisabled(),true);
    await page.evaluate(()=>{window.testFailures['plugin:dialog|open']='Folder unavailable';});
    await page.locator('#onboardingChooseFolder').click();
    await page.locator('#onboardingSetupStatus').filter({hasText:'Folder unavailable'}).waitFor();
    assert.equal(await page.locator('#onboardingSetupStatus').isVisible(),true);
    await page.evaluate(()=>{window.testFailures={};});
    await page.locator('#onboardingChooseFolder').click();
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingStepLabel').filter({hasText:'6 of 6'}).waitFor();
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-calendar-${viewport.name}.png`,output))});
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingOverlay').waitFor({state:'hidden'});
    await page.locator('#sessionPlanForm').waitFor({state:'visible'});
    await page.locator('#jiraSelect').selectOption('STUDY');
    assert.equal(await page.locator('#jiraSelect').isVisible(), true, 'Local task options must be visible on free and Pro plans.');
    assert.equal(await page.locator('#jiraSelect option:checked').innerText(), 'Study');
    await page.screenshot({path:fileURLToPath(new URL(`study-task-${viewport.name}.png`,output))});
    await page.locator('#navProfile').click();
    await page.locator('#languageSelect').selectOption('es');
    await page.waitForFunction(() => document.querySelector('#jiraSelect option:checked')?.textContent.trim() === 'Estudio');
    await page.locator('#navProfile').click();
    await page.locator('#languageSelect').selectOption('en');
    await page.locator('#navToday').click();
    assert.equal(await page.locator('#jiraSelect').inputValue(), 'STUDY', 'Language changes must preserve the selected task.');
    await page.locator('#jiraSelect').selectOption('General');
    const nativeInvoke = await page.evaluate(()=>{window.testOriginalInvoke=window.__TAURI_INTERNALS__.invoke; return true;});
    assert.equal(nativeInvoke,true);
    await page.locator('#sessionIntention').fill('Write proposal, 60 min; review proposal, 40 min.');
    await page.locator('#sessionStart').fill('09:00');await page.locator('#sessionEnd').fill('11:00');
    await page.locator('#sessionGenerate').click();
    await page.locator('#sessionProposal').waitFor({state:'visible'});
    assert.equal((await page.evaluate(()=>window.testCalls)).filter(c=>c.command==='confirm_session_plan').length,0);
    await page.locator('#sessionFeedback').fill('Review first, then write.');await page.locator('#sessionRevise').click();
    await page.getByText('Review first, as requested.',{exact:true}).waitFor();
    await page.locator('#sessionConfirm').scrollIntoViewIfNeeded();
    await page.screenshot({path:fileURLToPath(new URL(`session-proposal-${viewport.name}-actions.png`,output))});
    await page.locator('#sessionProposal').evaluate((element) => element.scrollIntoView({block:'start'}));
    await page.screenshot({path:fileURLToPath(new URL(`session-proposal-${viewport.name}.png`,output))});
    await page.locator('#sessionConfirm').click();
    await page.getByText('2 blocks added to FlowSight.',{exact:true}).waitFor();
    assert.equal((await page.evaluate(()=>window.testCalls)).filter(c=>c.command==='confirm_session_plan').length,1);
    await page.locator('#sessionCalendar').scrollIntoViewIfNeeded();
    await page.screenshot({path:fileURLToPath(new URL(`session-calendar-${viewport.name}.png`,output))});
    const calls=await page.evaluate(()=>window.testCalls);
    assert.equal(calls.some(c=>c.command==='start_total_focus'),false,'Wizard must save settings without activating protection.');
    assert.deepEqual(calls.find(c=>c.command==='save_total_focus_preferences').args.preferences.exceptions,['youtube.com/watch']);
    assert.equal(calls.find(c=>c.command==='save_total_focus_preferences').args.preferences.quietNotifications,false);
    assert.equal(calls.find(c=>c.command==='save_total_focus_preferences').args.preferences.musicEnabled,true);
    assert.equal(calls.find(c=>c.command==='save_total_focus_preferences').args.preferences.musicUrl,musicUrl);
    assert.equal(calls.find(c=>c.command==='set_focus_alerts_enabled').args.enabled,true);
    assert.equal(calls.find(c=>c.command==='save_weekly_report_schedule').args.schedule.enabled,true);
    assert.equal(calls.some(c=>c.command==='start_monitoring'||c.command==='set_calendar_auto_publish'||c.command==='set_analytics_consent'),false);
    await page.locator('#todayTotalFocus').click();
    await page.locator('#totalFocusSettings').waitFor({state:'visible'});
    assert.equal(await page.locator('#totalFocusStart').isDisabled(),true);
    await page.evaluate(async()=>{await window.__TAURI_INTERNALS__.invoke('test_focus_connection',{connected:true,available:false});});
    await page.locator('#totalFocusStatus').filter({hasText:'website protection unavailable'}).waitFor();
    await page.locator('#totalFocusRepair').waitFor({state:'visible'});
    await page.locator('#totalFocusRepairOpen').click();
    assert.equal((await page.evaluate(()=>window.testCalls)).some(c=>c.command==='open_browser_extension_folder'),true);
    assert.equal(await page.locator('#totalFocusStart').isDisabled(),true, 'Legacy extension must not start an unprotected session');
    await page.evaluate(async()=>{await window.__TAURI_INTERNALS__.invoke('test_focus_connection',{connected:true,available:true});});
    await page.locator('#totalFocusStart').waitFor({state:'visible'});
    await page.waitForFunction(()=>!document.getElementById('totalFocusStart').disabled);
    await page.evaluate(()=>{document.getElementById('totalFocusMusicEnabled').checked=false;document.getElementById('totalFocusMusicEnabled').dispatchEvent(new Event('input',{bubbles:true}));});
    await page.locator('#totalFocusRepair').waitFor({state:'hidden'});
    await page.locator('#totalFocusTask').fill('');
    assert.equal(await page.locator('#totalFocusTask').getAttribute('required'),null);
    assert.equal(await page.locator('label[for=totalFocusTask]').innerText(),'Your focus task (optional)');
    await page.locator('#totalFocusStart').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Your session is ready'}).waitFor();
    assert.match(await page.locator('#totalFocusActiveTask').innerText(),/^General ·/);
    assert.equal(await page.locator('#totalFocusTaskError').isVisible(),false);
    await page.locator('#totalFocusEnd').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Total focus ended'}).waitFor();
    await page.locator('#totalFocusTask').fill('   ');
    await page.locator('#totalFocusStart').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Your session is ready'}).waitFor();
    assert.match(await page.locator('#totalFocusActiveTask').innerText(),/^General ·/);
    await page.locator('#totalFocusEnd').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Total focus ended'}).waitFor();
    const startsBeforeInvalidDuration=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').length);
    await page.locator('#totalFocusTask').fill('ADDA · exercise 1');
    await page.locator('#totalFocusTaskError').waitFor({state:'hidden'});
    await page.locator('#totalFocusMinutes').fill('4');
    await page.locator('#totalFocusStart').click();
    assert.equal(await page.evaluate(()=>document.activeElement.id),'totalFocusMinutes');
    assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').length),startsBeforeInvalidDuration);
    await page.locator('#totalFocusMinutes').fill('50');
    await page.locator('#totalFocusQuiet').check();
    await page.evaluate(()=>{window.testDelays={start_total_focus:700};window.testFailures.start_total_focus='Browser unavailable';});
    await page.locator('#totalFocusStart').click();
    assert.equal(await page.locator('#totalFocusStart').innerText(),'Starting total focus…');
    assert.equal(await page.locator('#totalFocusStart').isDisabled(),true);
    assert.equal(await page.locator('#totalFocusTask').isDisabled(),true);
    await page.locator('#totalFocusFeedback').filter({hasText:'Waiting for your browser'}).waitFor();
    await page.locator('#totalFocusFeedback').filter({hasText:'Browser unavailable'}).waitFor();
    assert.equal(await page.locator('#totalFocusStart').isEnabled(),true);
    assert.equal(await page.locator('#totalFocusStart').innerText(),'Start total focus');
    assert.equal(await page.locator('#totalFocusFeedback').evaluate(element=>{
      const r=element.getBoundingClientRect();return r.top>=0 && r.bottom<=innerHeight;
    }),true,'Failed activation must leave a visible error and a retryable button.');
    await page.evaluate(()=>{window.testFailures={};window.testDelays={start_total_focus:700};});
    await page.locator('#totalFocusStart').click();
    assert.equal(await page.locator('#totalFocusStart').innerText(),'Starting total focus…');
    await page.locator('#totalFocusStatus').filter({hasText:'browser block confirmed'}).waitFor();
    await page.locator('#totalFocusQuietStatus').filter({hasText:'notification banners are silenced'}).waitFor();
    await page.locator('#totalFocusClock').click();
    await page.locator('#playTimerBtn').filter({hasText:'Pause total focus'}).waitFor();
    await inspectSpanishClock(page,viewport);
    const beforeCount=await page.locator('#timerDisplay').innerText();await page.waitForTimeout(1200);
    assert.notEqual(await page.locator('#timerDisplay').innerText(),beforeCount,'Total focus must count in the main clock.');
    await page.locator('#playTimerBtn').click();
    await page.locator('#playTimerBtn').filter({hasText:'Resume total focus'}).waitFor();
    await inspectSpanishClock(page,viewport,true);
    const pausedCount=await page.locator('#timerDisplay').innerText();await page.waitForTimeout(1200);
    assert.equal(await page.locator('#timerDisplay').innerText(),pausedCount,'The main clock must stay still during total focus pause.');
    assert.match(await page.locator('#timerRecordingNote').innerText(),/00:48:20.*Paused/);
    await page.locator('#playTimerBtn').click();
    await page.locator('#playTimerBtn').filter({hasText:'Pause total focus'}).waitFor();
    await page.locator('#todayTotalFocus').click();
    await page.locator('#totalFocusQuietStatus').filter({hasText:'notification banners are silenced'}).waitFor();
    assert.equal(await page.locator('#totalFocusDigest').isVisible(),false);
    await page.locator('#totalFocusSettings').evaluate(element=>element.scrollIntoView({block:'start'}));
    await page.screenshot({path:fileURLToPath(new URL(`total-focus-active-${viewport.name}.png`,output))});
    await page.locator('#totalFocusEnd').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Total focus ended'}).waitFor();
    await page.locator('#totalFocusDigestItems').filter({hasText:'Saved during focus'}).waitFor();
    await page.locator('#totalFocusDigestDismiss').click();
    await page.locator('#totalFocusDigest').waitFor({state:'hidden'});
    assert.equal((await page.evaluate(()=>window.testCalls)).some(c=>/start_monitoring|messages.*send/.test(c.command)),false);
    // Today starts with the current Settings edits, including a music link that
    // has not been saved. A cancelled consent must not save or start anything.
    await page.locator('#totalFocusSites').fill('youtube.com, instagram.com');
    await page.locator('#totalFocusMinutes').fill('35');
    await page.locator('#totalFocusQuiet').uncheck();
    await page.locator('#totalFocusMusicEnabled').check();
    const editedMusicUrl='https://music.example.com/listen?playlist=new&track=456';
    await page.locator('#totalFocusMusicUrl').fill(editedMusicUrl);
    await page.locator('#navToday').click();
    await page.locator('#manualTask').fill('Review the proposal');
    assert.equal(await page.locator('#todayTotalFocusLabel').innerText(),'Total focus settings');
    assert.equal(await page.locator('#todayTotalFocusStartLabel').innerText(),'Start total focus');
    assert.equal(await page.locator('#todayTotalFocusStart').evaluate(element=>element.getBoundingClientRect().top > document.getElementById('playTimerBtn').getBoundingClientRect().bottom),true);
    await page.locator('#todayTotalFocusStart').click();
    await page.locator('#monitoringNoticeOverlay.visible').waitFor();
    const startsBeforeConsent=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').length);
    const savesBeforeConsent=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='save_total_focus_preferences').length);
    await page.locator('#monitoringNoticeCancelBtn').click();
    assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').length),startsBeforeConsent);
    assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.command==='save_total_focus_preferences').length),savesBeforeConsent);
    await page.locator('#todayTotalFocusStart').click();
    await page.locator('#monitoringNoticeAcceptBtn').click();
    await page.locator('#todayTotalFocusFeedback').filter({hasText:'Your session is ready'}).waitFor();
    assert.equal(await page.locator('#tabToday').evaluate(element=>element.classList.contains('active')),true);
    const shortcutStart=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').at(-1));
    assert.equal(shortcutStart.args.intention,'Review the proposal');
    assert.deepEqual(shortcutStart.args.preferences.patterns,['youtube.com','instagram.com']);
    assert.equal(shortcutStart.args.preferences.durationMinutes,35);
    assert.equal(shortcutStart.args.preferences.quietNotifications,false);
    assert.equal(shortcutStart.args.preferences.musicEnabled,true);
    assert.equal(shortcutStart.args.preferences.musicUrl,editedMusicUrl);
    const savedAfterShortcut=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='save_total_focus_preferences').at(-1));
    assert.deepEqual(savedAfterShortcut.args.preferences,shortcutStart.args.preferences);
    await page.locator('#playTimerBtn').filter({hasText:'Pause total focus'}).waitFor();
    assert.equal(await page.locator('#todayTotalFocusStart').isVisible(),false);
    await page.locator('#stopTimerBtn').click();
    await page.locator('#todayTotalFocusStart').waitFor({state:'visible'});
    // With no dirty Settings fields, Today reuses the preferences just saved.
    await page.locator('#manualTask').fill('');
    await page.locator('#todayTotalFocusStart').click();
    await page.locator('#todayTotalFocusFeedback').filter({hasText:'Your session is ready'}).waitFor();
    const emptyTodayStart=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='start_total_focus').at(-1));
    assert.equal(emptyTodayStart.args.intention,'General');
    assert.deepEqual(emptyTodayStart.args.preferences,shortcutStart.args.preferences);
    await page.locator('#stopTimerBtn').click();
    await page.locator('#todayTotalFocusStart').waitFor({state:'visible'});
    await page.evaluate(()=>{window.testFailures={start_total_focus:'Browser did not confirm'};});
    await page.locator('#todayTotalFocusStart').click();
    await page.locator('#todayTotalFocusFeedback').filter({hasText:'Browser did not confirm'}).waitFor();
    assert.equal(await page.locator('#todayTotalFocusStart').isEnabled(),true);
    assert.equal(await page.locator('#tabToday').evaluate(element=>element.classList.contains('active')),true);
    await page.evaluate(()=>{window.testFailures={};});
    await page.evaluate(()=>{document.getElementById('todayTotalFocusFeedback').hidden=true;document.getElementById('sessionPlanner').open=false;});
    await page.locator('#todayTotalFocusStart').scrollIntoViewIfNeeded();
    await page.screenshot({path:fileURLToPath(new URL(`total-focus-quick-start-${viewport.name}.png`,output))});
    // Spanish labels must fit as well as the English controls.
    await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value='es';select.dispatchEvent(new Event('change',{bubbles:true}));});
    assert.equal(await page.locator('#todayTotalFocusStartLabel').innerText(),'Iniciar concentración total');
    assert.equal(await page.locator('#todayTotalFocusLabel').innerText(),'Ajustes concentración total');
    await assertTimerLabelFits(page,'#playTimerBtn');
    await assertTimerLabelFits(page,'#todayTotalFocusStart');
    const actionStyles=await page.locator('.timer-primary-btn').evaluateAll(buttons=>buttons.map(button=>{
      const css=getComputedStyle(button);return {background:css.backgroundImage,color:css.color,fontSize:css.fontSize,fontWeight:css.fontWeight,lineHeight:css.lineHeight,padding:css.padding,borderRadius:css.borderRadius,height:button.getBoundingClientRect().height};
    }));
    assert.deepEqual(actionStyles[0],actionStyles[1],'Tracking and total focus play must share their design and size.');
    await page.screenshot({path:fileURLToPath(new URL(`total-focus-quick-start-es-${viewport.name}.png`,output))});
    await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value='en';select.dispatchEvent(new Event('change',{bubbles:true}));});
    // Notion is absent for both free and paid entitlements and in both report
    // branches. No provider calls or erasure occur when visiting Insights.
    await page.locator('#navSummary').click();
    await page.getByRole('heading',{name:'Insights',exact:true}).waitFor();
    assert.equal(await page.locator('#notionReportBtn, #notionIntegrationModal').count(),0);
    await page.screenshot({path:fileURLToPath(new URL(`insights-notion-disabled-${viewport.name}.png`,output))});
    await page.evaluate(()=>{window.testRecordedHistory=true;});
    await page.locator('#navToday').click();await page.locator('#navSummary').click();
    await page.getByText('Synthetic writing session',{exact:true}).waitFor();
    assert.equal(await page.locator('#notionReportBtn, #notionIntegrationModal').count(),0);
    assert.equal((await page.evaluate(()=>window.testCalls)).some(c=>/notion/.test(c.command)),false);
    await page.locator('#navToday').click();
    // A failed replacement must preserve the original draft's expiry timer.
    await page.evaluate(()=>{document.getElementById('sessionPlanner').open=true;});
    await page.evaluate(()=>{
      window.__TAURI_INTERNALS__.invoke=async(command,args)=>{
        if(command==='propose_session_plan'){
          if(args.feedback) throw new Error('Local model unavailable');
          const result=await window.testOriginalInvoke(command,args); return {...result,expiresInSeconds:.2};
        }
        return window.testOriginalInvoke(command,args);
      };
    });
    await page.locator('#sessionGenerate').click();
    await page.locator('#sessionProposal').waitFor({state:'visible'});
    await page.locator('#sessionFeedback').fill('Change the order.');await page.locator('#sessionRevise').click();
    await page.locator('#sessionPlanStatus').filter({hasText:'This draft expired'}).waitFor();
    assert.equal(await page.locator('#sessionConfirm').isDisabled(),true);
    const overflow=await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth);
    assert.equal(overflow,false);assert.deepEqual(errors,[]);assert.deepEqual(await page.evaluate(()=>window.testWindowErrors),[]);
    console.log(`${viewport.name}: optional empty focus task, current music settings saved and used by Today, consent cancel/accept, failure/retry, linked clock and Spanish labels passed.`);
    await page.locator('#todayTotalFocus').click();
    await page.evaluate(()=>{const select=document.getElementById('languageSelect');select.value='es';select.dispatchEvent(new Event('change',{bubbles:true}));});
    await page.locator('#totalFocusMusicEnabled').check();
    await page.locator('#totalFocusMusicUrl').fill(musicUrl);
    assert.equal(await page.locator('label[for=totalFocusMusicUrl]').innerText(),'Enlace de música');
    assert.equal(await page.locator('label[for=totalFocusTask]').innerText(),'Tu tarea de concentración (opcional)');
    assert.match(await page.locator('label[for=totalFocusMusicEnabled]').innerText(),/Abrir música al iniciar la concentración total/);
    assert.equal(await page.locator('#totalFocusMusicUrl').inputValue(),musicUrl);
    assert.equal(await page.locator('#totalFocusMusicUrl').evaluate(input=>input.getBoundingClientRect().width>200),true);
    await page.locator('label[for=totalFocusTask]').evaluate(label=>label.scrollIntoView({block:'start'}));
    await page.screenshot({path:fileURLToPath(new URL(`focus-music-es-${viewport.name}.png`,output)),animations:'disabled'});
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    console.log(`${viewport.name}: background music saved during setup, used by Today, preserved on language change, compact fields fit.`);
    await page.close();
  }
} finally { await browser.close(); }
