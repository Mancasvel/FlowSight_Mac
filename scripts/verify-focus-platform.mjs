// Real renderer; synthetic native capabilities isolate the platform UI contract.
import assert from 'node:assert/strict';
import {readFile,mkdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {chromium,webkit} from 'playwright';

const source=await readFile(new URL('./verify-session-onboarding.mjs',import.meta.url),'utf8');
const begin=source.indexOf('await page.addInitScript(')+'await page.addInitScript('.length;
const end=source.indexOf('}, { pro:Boolean(viewport.dark) });',begin)+1;
assert.ok(begin>0&&end>begin,'Shared native fixture must be available');
const initialize=source.slice(begin,end);
assert.ok(initialize.includes('systemNotificationsAvailable:true'));
const browserName=process.env.FLOWSIGHT_E2E_BROWSER||'chromium';
const browser=await (browserName==='webkit'?webkit:chromium).launch({headless:true});
const output=new URL(`../.impeccable/review/focus-platform-${browserName}/`,import.meta.url);
await mkdir(output,{recursive:true});
try {
  for(const available of [false,true]) {
    const page=await browser.newPage({viewport:{width:900,height:700},locale:'en-GB'});
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(eval(`(${initialize.replace('systemNotificationsAvailable:true',`systemNotificationsAvailable:${available}`)})`),{pro:false});
    await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1420',{waitUntil:'networkidle'});
    await page.locator('#chooseIndividualUsage').click();
    await page.locator('#onboardingOverlay.visible').waitFor();
    for(const heading of ['A plan you can change','Your reminders, your choice','Make space for total focus']) {
      await page.locator('#onboardingContinueBtn').click();
      if(heading==='Your reminders, your choice') await page.locator('#onboardingFocusReminders').waitFor();
      else await page.getByRole('heading',{name:heading}).waitFor();
    }
    assert.equal(await page.locator('#onboardingFocusQuietControl').isVisible(),available);
    assert.equal(await page.locator('#onboardingFocusQuietHelp').isVisible(),available);
    assert.equal(await page.locator('#onboardingFocusQuiet').isDisabled(),!available);
    assert.equal(await page.locator('#onboardingFocusQuiet').isChecked(),available);
    await page.screenshot({path:fileURLToPath(new URL(`onboarding-${available?'supported':'unsupported'}-simulated-native.png`,output))});
    await page.locator('#onboardingContinueBtn').click();
    await page.getByRole('heading',{name:'See what your week contained'}).waitFor();
    const saved=await page.evaluate(()=>window.testCalls.find(call=>call.command==='save_total_focus_preferences').args.preferences);
    assert.equal(saved.quietNotifications,available,'The wizard must preserve the capability while its controls are temporarily disabled for saving');
    await page.locator('#onboardingSkipCalendarBtn').click();
    await page.locator('#onboardingOverlay.visible').waitFor({state:'hidden'});
    await page.locator('#navProfile').click();
    await page.locator('#totalFocusSettings').waitFor({state:'visible'});
    assert.equal(await page.locator('#totalFocusQuietControl').isVisible(),available);
    assert.equal(await page.locator('#totalFocusQuietHelp').isVisible(),available);
    assert.equal(await page.locator('#totalFocusQuiet').isDisabled(),!available);
    assert.equal(await page.locator('#totalFocusQuietStatus').innerText(),'');
    assert.match(await page.locator('#totalFocusSettings').innerText(),/FlowSight focus reminders are held in your local digest/);
    await page.locator('#totalFocusSites').fill('distraction.example.com');
    await page.locator('#totalFocusSave').click();
    await page.locator('#totalFocusFeedback').filter({hasText:'Focus settings saved'}).waitFor();
    assert.equal(await page.evaluate(()=>window.testCalls.filter(call=>call.command==='save_total_focus_preferences').at(-1).args.preferences.quietNotifications),available);
    await page.locator('#totalFocusSettings').evaluate(element=>element.scrollIntoView({block:'start'}));
    await page.screenshot({path:fileURLToPath(new URL(`settings-${available?'supported':'unsupported'}-simulated-native.png`,output))});
    assert.deepEqual(errors,[]);
    await page.close();
  }
  console.log('PASS: platform capability controls the notification option and help in onboarding/settings; unsupported systems save no OS mute request, while own reminders remain available. Native service is simulated.');
} finally {await browser.close();}
