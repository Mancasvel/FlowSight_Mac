// One-time port of the reviewed session/setup surface onto the 5.0.3 renderer.
// Platform capture, runtime, auth, report renderer and window configuration stay native.
import { readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import path from 'node:path';
const source=process.argv[2];
if(!source)throw new Error('Provide the reviewed Windows source directory.');
const target='apps/agent/src/renderer/index.html';
const original=readFileSync(target,'utf8').replaceAll('\r\n','\n');let html=original;
const windows=readFileSync(path.join(source,target),'utf8').replaceAll('\r\n','\n');
const slice=(text,start,end)=>text.slice(text.indexOf(start),text.indexOf(end,text.indexOf(start)));
const replace=(start,end,value)=>{if(!html.includes(start)||!html.includes(end))throw new Error(`Missing port marker ${start}`);html=html.replace(slice(html,start,end),value);};
replace('  <!-- ONBOARDING WIZARD','  <!-- ANALYTICS CONSENT',slice(windows,'  <!-- ONBOARDING WIZARD','  <!-- ANALYTICS CONSENT'));
replace('    const onboardingState =','    function onboardingRoleLabel',slice(windows,'    const onboardingState =','    function onboardingRoleLabel'));
let setup=slice(windows,'    function onboardingCanContinue()','    const PRIVACY_NOTICE_VERSION');
// The existing platform has no external calendar companion yet; offer the four
// implemented setup steps instead of advertising unavailable controls.
const calendarStart=setup.indexOf('      } else if (step === 4) {');
const calendarEnd=setup.indexOf('      body.scrollTop = 0;',calendarStart);
setup=setup.slice(0,calendarStart)+'      }\n'+setup.slice(calendarEnd);
setup=setup.replaceAll('/ 5)','/ 4)').replaceAll('${step + 1} of 5','${step + 1} of 4')
  .replaceAll("step === 4 ? 'Finish setup'","step === 3 ? 'Finish setup'")
  .replaceAll('onboardingState.step === 4 ?','onboardingState.step === 3 ?')
  .replaceAll('onboardingState.step === 4) await finishOnboarding()','onboardingState.step === 3) await finishOnboarding()')
  .replaceAll('      loadCalendarStatus().catch(() => {});','');
replace('    function onboardingCanContinue()','    let analyticsConsent =',setup+'\n    let desktopPreferences = null;\n    let weeklyReportSchedule = null;\n    async function loadDesktopPreferences() {\n      desktopPreferences = await invoke(\'get_desktop_preferences\');\n      const focus = document.getElementById(\'focusAlertsToggle\');\n      const context = document.getElementById(\'contextualFocusAlertsToggle\');\n      if(focus) focus.checked = Boolean(desktopPreferences.focusAlertsEnabled);\n      if(context) { context.checked=Boolean(desktopPreferences.contextualFocusAlertsEnabled); context.disabled=!desktopPreferences.focusAlertsEnabled; }\n    }\n    function renderWeeklyReportForm(schedule) {\n      weeklyReportSchedule = schedule;\n    }\n');
const planner=slice(windows,'        <details class="session-planner"','        <div class="today-preferences">');
const timerEnd=html.indexOf('        <div class="today-preferences">');
if(timerEnd<0)throw new Error('Missing Today anchor');html=html.slice(0,timerEnd)+planner+html.slice(timerEnd);
html=html.replace("    import { invoke }", "    import './session-planner.css';\n    import { mountSessionPlanner } from './session-planner.mjs';\n    import { isWeeklyReportDue } from './weekly-report-schedule.mjs';\n    import { open as openFolderDialog } from '@tauri-apps/plugin-dialog';\n    import { invoke }");
html=html.replace('    async function init() {','    const sessionPlanner = mountSessionPlanner({ invoke });\n\n    async function init() {');
html=html.replace('      await loadUserPreferences();\n      showMainApp();','      await loadUserPreferences();\n      await loadDesktopPreferences();\n      weeklyReportSchedule = await invoke(\'get_weekly_report_schedule\');\n      showMainApp();');
html=html.replace('      // Background self-update check;',"      sessionPlanner.refresh();\n      setInterval(() => checkPortableWeeklyReport().catch(console.warn), 30000);\n      checkPortableWeeklyReport().catch(console.warn);\n\n      // Background self-update check;");
// Persist/save hooks remain explicit and independent of the visual example.
const portable=`
    let portableWeeklyReportBusy = false;
    async function checkPortableWeeklyReport() {
      if(portableWeeklyReportBusy) return;
      const schedule = await invoke('get_weekly_report_schedule');
      if(!isWeeklyReportDue(schedule,new Date())) return;
      portableWeeklyReportBusy = true;
      const startedAt = new Date().toISOString();
      try {
      const payload = await invoke('generate_local_status_report');
        const model = createStatusReportViewModel(payload,{userName:currentUser?.display_name||'User',todayDate:currentHistoryData?.date});
        const doc = renderStatusReportPdf(new jsPDF(),model);
        const filename = buildStatusReportPdfFilename(payload,currentHistoryData);
        await invoke('save_scheduled_report_pdf',{revision:schedule.revision,startedAt,filename,bytes:Array.from(new Uint8Array(doc.output('arraybuffer')))});
        weeklyReportSchedule = await invoke('get_weekly_report_schedule');
      } finally { portableWeeklyReportBusy=false; }
    }
    document.getElementById('focusAlertsToggle').addEventListener('change',async(e)=>{
      try { await invoke('set_focus_alerts_enabled',{enabled:e.target.checked}); await loadDesktopPreferences(); }
      catch(error) { showToast(String(error),'error'); await loadDesktopPreferences(); }
    });
    document.getElementById('contextualFocusAlertsToggle').addEventListener('change',async(e)=>{
      try { await invoke('set_contextual_focus_alerts_enabled',{enabled:e.target.checked}); await loadDesktopPreferences(); }
      catch(error) { showToast(String(error),'error'); await loadDesktopPreferences(); }
    });
`;
html=html.replace('    let analyticsConsent =',portable+'\n    let analyticsConsent =');
const focusCard=`
        <div class="card" id="focusRemindersCard">
          <div class="card-header"><div class="card-title">Focus reminders</div></div>
          <label class="consent-toggle-row"><input type="checkbox" id="focusAlertsToggle"><span>Enable focus reminders while tracking</span></label>
          <label class="consent-toggle-row"><input type="checkbox" id="contextualFocusAlertsToggle"><span>Include recent work context in reminders</span></label>
          <p class="onboarding-prefs-summary">Optional. Generic reminders stay free of task details. Context can include your selected task in a desktop notification.</p>
        </div>
`;
html=html.replace('        <div class="card" id="privacyAnalyticsCard">',focusCard+'        <div class="card" id="privacyAnalyticsCard">');
if(!html.includes('id="focusRemindersCard"'))throw new Error('Missing reminder settings anchor');
writeFileSync(target,html);
for(const file of ['session-planner.css','session-planner.mjs','session-planner.test.mjs','weekly-report-schedule.mjs'])copyFileSync(path.join(source,'apps/agent/src/renderer',file),path.join('apps/agent/src/renderer',file));
const darkFile='apps/agent/src/renderer/public/theme-dark-mobile.css';
const dark=readFileSync(path.join(source,darkFile),'utf8');
writeFileSync(darkFile,dark);
console.log('Ported local session planning, four optional setup steps, notification examples and dark hover corrections.');
