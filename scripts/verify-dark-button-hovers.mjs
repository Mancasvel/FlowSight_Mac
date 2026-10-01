// Actual renderer state/hover checks with synthetic native responses only.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const stage=process.argv.includes('--capture-before')?'before':'after';
const output=new URL('../.impeccable/review/',import.meta.url);
await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});
const measurements=[];

function channels(color) { return color.match(/[\d.]+/g)?.slice(0,3).map(Number) || [0,0,0]; }
function lightness(color) { const c=channels(color); return (Math.max(...c)+Math.min(...c))/510; }
function luminance(color) {
  const c=channels(color).map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);
  return c[0]*.2126+c[1]*.7152+c[2]*.0722;
}
function contrast(a,b) { const l=[luminance(a),luminance(b)].sort((a,b)=>b-a); return (l[0]+.05)/(l[1]+.05); }

async function inspectButton(page,selector,name,{minimumContrast=4.5}={}) {
  const button=page.locator(selector);
  await button.scrollIntoViewIfNeeded();
  await page.mouse.move(1,1);await button.evaluate(b=>b.blur());
  await page.waitForTimeout(180);
  const read=()=>button.evaluate(b=>{
    const css=getComputedStyle(b);let background=css.backgroundColor;
    if(background==='rgba(0, 0, 0, 0)') {
      let ancestor=b.parentElement;
      while(ancestor && getComputedStyle(ancestor).backgroundColor==='rgba(0, 0, 0, 0)')ancestor=ancestor.parentElement;
      background=ancestor?getComputedStyle(ancestor).backgroundColor:'rgb(16, 29, 37)';
    }
    return {background,foreground:css.color,outline:css.outlineStyle};
  });
  const base=await read();
  await page.screenshot({path:fileURLToPath(new URL(`dark-hover-${name}-${stage}-rest.png`,output))});
  await button.hover();await page.waitForTimeout(180);const hover=await read();
  await page.screenshot({path:fileURLToPath(new URL(`dark-hover-${name}-${stage}.png`,output))});
  const delta=lightness(hover.background)-lightness(base.background);
  measurements.push({name,base,hover,lightnessDelta:delta,contrast:contrast(hover.foreground,hover.background)});
  if(stage==='after') {
    assert.ok(delta>=-.005 && delta<=.08,`${name} hover must slightly lighten its base: ${JSON.stringify({base,hover,delta})}`);
    assert.ok(lightness(hover.background)<.4,`${name} must remain dark during hover.`);
    assert.ok(contrast(hover.foreground,hover.background)>=minimumContrast,`${name} hover foreground must retain readable contrast.`);
  }
  await page.mouse.move(1,1);await page.keyboard.press('Tab');await button.focus();const focused=await read();
  if(stage==='after') {
    assert.ok(lightness(focused.background)<.4,`${name} focus must remain dark.`);
    assert.equal(focused.outline,'solid',`${name} retains its keyboard focus outline.`);
    await button.evaluate(b=>{b.disabled=true;b.blur();});await button.hover({force:true});await page.waitForTimeout(180);
    const disabled=await read();
    assert.equal(disabled.background,base.background,`${name} disabled controls must not pick up a hover fill.`);
    await button.evaluate(b=>{b.disabled=false;});
  }
}

try {
  for(const viewport of [{width:370,height:700,name:'compact'},{width:900,height:800,name:'wide'}]) {
    const page=await browser.newPage({viewport:{width:viewport.width,height:viewport.height},colorScheme:'dark',reducedMotion:'reduce',timezoneId:'Europe/Madrid'});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.clock.setFixedTime(new Date('2026-10-01T10:00:00+02:00'));
    await page.addInitScript(()=>{
      window.testCalls=[];const callbacks=new Map();let counter=1;
      const prefs={onboardingCompleted:false,displayName:'',workRoles:[],workActivities:[],improvementGoals:[],dailyGoalHours:6};
      const history={date:'2026-10-01',total_seconds:0,entries:[],category_breakdown:[],ticket_breakdown:[],focus:{}};
      const responses={initialize_agent:null,get_config:{captureInterval:60000,dailyGoalHours:6},get_auth_session:null,get_current_user:null,
        get_entitlements:{plan:'free',status:'active',can_integrations:false,can_cloud_ai:false,can_sync:false,team_ids:[]},
        get_privacy_settings:{monitoringNoticeAcknowledged:true,cloudSyncEnabled:false,cloudAiEnabled:false,storeWindowTitles:false,excludedApplications:[],retentionDays:30},
        get_analytics_consent:{decided:true,consented:false},check_ollama:{online:false},get_status:{isRunning:true},check_installation_health:{healthy:true},check_local_server:{online:false},
        get_week_summary:{days:[]},get_today_history:history,get_local_agent_data:{events:[],preferences:{},tasks:[]},
        get_calendar_companion_status:{googleConnected:false,microsoftConnected:false,googleAvailable:false,microsoftAvailable:false,current:null},
        get_mcp_connection_info:{command:'/Example/FlowSight'},get_user_preferences:prefs,get_desktop_preferences:{focusAlertsEnabled:false,contextualFocusAlertsEnabled:false,promptDecided:true},
        get_weekly_report_schedule:{enabled:false,weekday:5,time:'17:00',folder:'',revision:0},get_browser_pairing:{connected:false}};
      window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
      window.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{windowLabel:'main',label:'main'}},
        transformCallback(fn){const id=counter++;callbacks.set(id,fn);return id;},unregisterCallback(id){callbacks.delete(id);},convertFileSrc(p){return p;},
        async invoke(command,args={}) {
          window.testCalls.push({command,args});
          if(command==='save_user_preferences_command')return {...args.prefs,onboardingCompleted:true};
          if(command.startsWith('plugin:window|'))return command.endsWith('is_maximized')?false:null;
          if(command==='plugin:event|listen')return args.handler;
          if(command.startsWith('plugin:event|')||command.startsWith('plugin:updater|'))return null;
          if(command==='plugin:app|version')return '5.0.10';
          return command in responses?structuredClone(responses[command]):null;
        }};
    });
    await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1432',{waitUntil:'networkidle'});
    await page.locator('#onboardingOverlay.visible').waitFor();await page.evaluate(()=>document.fonts.ready);
    await page.locator('#onboardingContinueBtn').click();await page.locator('#onboardingContinueBtn').click();await page.locator('#onboardingContinueBtn').click();
    await inspectButton(page,'#onboardingChooseFolder',`onboarding-secondary-${viewport.name}`);
    if(stage==='after')await inspectButton(page,'#onboardingContinueBtn',`onboarding-primary-${viewport.name}`);
    await page.locator('#onboardingSkipCalendarBtn').click();await page.locator('#onboardingOverlay').waitFor({state:'hidden'});
    // Display the actual stop control in a synthetic tracking visual state.
    // Tracking remains off and no native start/stop command is invoked.
    await page.locator('#stopTimerBtn').evaluate(button => { button.style.display='flex'; button.disabled=false; });
    await inspectButton(page,'#stopTimerBtn',`stop-${viewport.name}`);
    await page.locator('#navSummary').click();await page.locator('#generateReportBtn').waitFor();
    await inspectButton(page,'#generateReportBtn',`work-report-${viewport.name}`);
    await page.locator('#navProfile').click();
    await page.locator('#showMcpConnectionBtn').click();
    await inspectButton(page,'#copyMcpCommandBtn',`settings-ghost-${viewport.name}`);
    assert.deepEqual(errors,[]);
    assert.equal((await page.evaluate(()=>window.testCalls)).some(c=>c.command==='start_monitoring'||c.command==='stop_monitoring'||c.command==='propose_session_plan'),false);
    await page.close();
  }
  await writeFile(new URL(`dark-button-hovers-${stage}.json`,output),JSON.stringify(measurements,null,2)+'\n');
  console.log(`${stage}: ${measurements.length} actual dark-button hover states captured${stage==='after'?' and passed subdued lightness/contrast checks':''}.`);
} finally { await browser.close(); }
