// Browser integration checks against the actual renderer with isolated synthetic
// native responses. No user's local database, tracking, or calendars are changed.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const output = new URL('../.impeccable/review/', import.meta.url);
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
try {
  for (const viewport of [{width:370,height:700,name:'compact'},{width:340,height:400,name:'small'},{width:900,height:800,name:'wide-dark',dark:true}]) {
    const page = await browser.newPage({viewport:{width:viewport.width,height:viewport.height},timezoneId:'Europe/Madrid',locale:'en-GB',colorScheme:viewport.dark?'dark':'light',reducedMotion:'reduce'});
    await page.clock.setFixedTime(new Date('2026-10-01T08:00:00+02:00'));
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(({ pro }) => {
      const calls=[];window.testCalls=calls;window.testWindowErrors=[];
      window.addEventListener('error',event=>window.testWindowErrors.push(event.message));
      window.testFailures={};
      let events=[],proposalCount=0;
      let prefs={onboardingCompleted:false,displayName:'',workRoles:[],workActivities:[],improvementGoals:[],dailyGoalHours:6};
      let desktop={focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true};
      let schedule={enabled:false,weekday:5,time:'17:00',folder:'',revision:0};
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
    await page.locator('#onboardingWeeklyEnabled').check();
    assert.equal(await page.locator('#onboardingContinueBtn').isDisabled(),true);
    await page.evaluate(()=>{window.testFailures['plugin:dialog|open']='Folder unavailable';});
    await page.locator('#onboardingChooseFolder').click();
    await page.locator('#onboardingSetupStatus').filter({hasText:'Folder unavailable'}).waitFor();
    assert.equal(await page.locator('#onboardingSetupStatus').isVisible(),true);
    await page.evaluate(()=>{window.testFailures={};});
    await page.locator('#onboardingChooseFolder').click();
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingStepLabel').filter({hasText:'5 of 5'}).waitFor();
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-calendar-${viewport.name}.png`,output))});
    await page.locator('#onboardingContinueBtn').click();
    await page.locator('#onboardingOverlay').waitFor({state:'hidden'});
    await page.locator('#sessionPlanForm').waitFor({state:'visible'});
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
    assert.equal(calls.find(c=>c.command==='set_focus_alerts_enabled').args.enabled,true);
    assert.equal(calls.find(c=>c.command==='save_weekly_report_schedule').args.schedule.enabled,true);
    assert.equal(calls.some(c=>c.command==='start_monitoring'||c.command==='set_calendar_auto_publish'||c.command==='set_analytics_consent'),false);
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
    console.log(`${viewport.name}: block-label containment, notification examples/consents, Notion disabled, optional onboarding, revision without writes, and one confirmed save passed.`);
    await page.close();
  }
} finally { await browser.close(); }
