// Production renderer exercised with isolated, fictional Tauri IPC. No user account or screen data.
import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium, webkit } from 'playwright';

const source = await readFile(new URL('./verify-recorded-time.mjs', import.meta.url), 'utf8');
const start = source.indexOf('await page.addInitScript(') + 'await page.addInitScript('.length;
const end = source.indexOf('}, { locale });', start) + 1;
const initialize = source.slice(start, end);
const output = resolve(process.env.FLOWSIGHT_USAGE_EVIDENCE || '../team-profiles/verification');
await mkdir(output, { recursive: true });
const browserType = process.env.FLOWSIGHT_E2E_BROWSER === 'webkit' ? webkit : chromium;
const browser = await browserType.launch({ headless: true });
let currentPage;
const results = [];
const scenario = name => !process.env.FLOWSIGHT_USAGE_SCENARIO || process.env.FLOWSIGHT_USAGE_SCENARIO === name;

async function pageFor({ profile = 'team_employee', locale = 'en-GB', completed = true, signedIn = true, licensed = true, usageChoice = null, solo = false, stale = false, legacy = false, offline = false, pomodoro = false, dark = false, holdBoot = false } = {}) {
  const page = await browser.newPage({ locale, timezoneId: 'Europe/Madrid', colorScheme: dark ? 'dark' : 'light', viewport: { width: 370, height: 700 }, reducedMotion: 'reduce' });
  currentPage = page;
  page.setDefaultTimeout(30_000);
  await page.clock.install({ time: new Date('2026-10-01T17:00:00+02:00') });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript({ content: `(${initialize})({locale:${JSON.stringify(locale)}});` });
  await page.addInitScript(config => {
    const original = window.__TAURI_INTERNALS__.invoke;
    let running = false, milliseconds = 20_700_000, sampledAt = performance.now();
    const teams = [{ id: 'company-a', name: 'Acme Diseño', manager: false }, { id: 'company-b', name: 'Beta Engineering', manager: true }, { id: 'company-c', name: 'Gamma Design', manager: false }];
    const profileValue = (id, teamId) => ({
      id, verified: true, license_plan_id: id === 'individual' ? 'solo' : 'team_simple',
      team_id: id === 'individual' ? null : teamId || (id === 'team_manager' ? 'company-b' : 'company-a'), teams,
      available_profiles: ['team_employee', ...(config.solo || id === 'individual' ? ['individual'] : [])],
      capabilities: { online: id === 'individual', focus_alerts: id === 'individual', personal_insights: id === 'individual', team_dashboard: id === 'team_manager' },
    });
    const freeProfile = { id: 'individual', verified: true, license_plan_id: null, team_id: null, allowed_project_keys: [], available_profiles: ['individual'], teams: [], capabilities: { online: true, focus_alerts: true, personal_insights: true, team_dashboard: false } };
    window.usageFixture = {
      signedIn: config.signedIn, licensed: config.licensed, profile: config.licensed ? profileValue(config.profile) : structuredClone(freeProfile), failProfile: false, failJoin: null,
      preferences: { onboardingCompleted: config.completed, displayName: 'Synthetic employee', dailyGoalHours: 6, workRoles: [], workActivities: [], improvementGoals: [] },
      privacy: { noticeVersion: '2026-08-23', monitoringNoticeAcknowledged: config.completed, cloudSyncEnabled: false, cloudAiEnabled: false, storeWindowTitles: false, excludedApplications: [], retentionDays: 30 },
    };
    const snapshot = () => {
      const tick = performance.now(); if (running) milliseconds += Math.max(0, tick - sampledAt); sampledAt = tick;
      return { date: '2026-10-01', total_seconds: Math.floor(milliseconds / 1000), total_milliseconds: milliseconds, is_running: running };
    };
    const entitlements = () => !window.usageFixture.signedIn ? { owner_user_id: null, plan: null, status: 'free', team_ids: [] } : !window.usageFixture.licensed ? {
      owner_user_id: 'employee-ui', plan: null, status: 'free', team_ids: [], active_team_id: null, can_sync: false, can_cloud_ai: false, can_integrations: false, usage_profile: structuredClone(freeProfile),
    } : {
      owner_user_id: config.stale ? 'another-account' : 'employee-ui', plan: window.usageFixture.profile.id === 'individual' ? 'individual' : 'team', status: 'active', can_sync: true, can_cloud_ai: true, can_integrations: true,
      team_ids: teams.map(team => team.id), active_team_id: window.usageFixture.profile.team_id,
      usage_profile: config.legacy ? null : structuredClone(window.usageFixture.profile),
    };
    if (config.pomodoro) localStorage.setItem('flowsight_timer_modes_v1', JSON.stringify({ mode: 'pomodoro', work: 25, shortBreak: 5, longBreak: 15 }));
    if (config.usageChoice) localStorage.setItem('flowsight_usage_onboarding_choice', config.usageChoice);
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      const fixture = window.usageFixture;
      if (command === 'initialize_agent' && config.holdBoot) return new Promise(resolve => { fixture.releaseBoot = () => resolve(true); });
      if (!new Set(['get_current_user', 'get_auth_session', 'get_entitlements', 'refresh_entitlements', 'get_user_teams', 'get_user_preferences', 'save_user_preferences_command', 'get_privacy_settings', 'update_privacy_settings', 'get_status', 'get_tracking_clock', 'get_today_history', 'start_monitoring', 'stop_monitoring', 'get_desktop_preferences', 'local_model_status', 'start_server', 'set_usage_profile', 'claim_license_code', 'join_team', 'login_with_code', 'logout', 'clear_user_session', 'get_online_leagues']).has(command)) return original(command, args);
      window.testCalls.push({ command, args });
      if (command === 'get_current_user') return fixture.signedIn ? { user_id: 'employee-ui', email: 'synthetic@example.test', team_id: fixture.profile.team_id } : null;
      if (command === 'get_auth_session') return null;
      if (command === 'refresh_entitlements' && config.offline) throw 'Synthetic network unavailable';
      if (command === 'get_entitlements' || command === 'refresh_entitlements') return entitlements();
      if (command === 'get_user_teams') return { teams: fixture.licensed ? teams.map(team => ({ team_id: team.id, name: team.name, role: team.manager ? 'admin' : 'member' })) : [], active_team_id: fixture.profile.team_id };
      if (command === 'get_user_preferences') return structuredClone(fixture.preferences);
      if (command === 'save_user_preferences_command') { fixture.preferences = structuredClone(args.prefs); return structuredClone(fixture.preferences); }
      if (command === 'get_privacy_settings') return structuredClone(fixture.privacy);
      if (command === 'update_privacy_settings') { Object.assign(fixture.privacy, args.patch); return structuredClone(fixture.privacy); }
      if (command === 'get_status') return { isRunning: running };
      if (command === 'get_tracking_clock') return snapshot();
      if (command === 'get_today_history') { const history = await original(command, args); return { ...history, tracking: snapshot() }; }
      if (command === 'start_monitoring' || command === 'stop_monitoring') { snapshot(); running = command === 'start_monitoring'; return true; }
      if (command === 'get_desktop_preferences') return { launchAtLogin: true, autostartLaunch: true, startMonitoringAtLogin: true, focusAlertsEnabled: true, contextualFocusAlertsEnabled: true, promptDecided: true };
      if (command === 'local_model_status') return { ready: true };
      if (command === 'start_server') return { status: 'already_running' };
      if (command === 'set_usage_profile') {
        if (fixture.failProfile) throw 'Profile access could not be verified';
        snapshot(); running = false; fixture.privacy.cloudSyncEnabled = fixture.privacy.cloudAiEnabled = false;
        fixture.profile = profileValue(args.mode === 'individual' ? 'individual' : teams.find(team => team.id === args.teamId)?.manager ? 'team_manager' : 'team_employee', args.teamId);
        return entitlements();
      }
      if (command === 'join_team' || command === 'claim_license_code') {
        if (command === 'join_team' && fixture.failJoin) throw fixture.failJoin;
        fixture.licensed = true; fixture.profile = profileValue('team_employee', 'company-a');
        running = false; fixture.privacy.cloudSyncEnabled = fixture.privacy.cloudAiEnabled = false;
        return command === 'claim_license_code' ? entitlements() : { team_id: 'company-a', team_name: teams[0].name };
      }
      if (command === 'login_with_code') { fixture.signedIn = true; return { provider: 'manual', user: { id: 'employee-ui', email: 'synthetic@example.test', display_name: 'Synthetic employee' } }; }
      if (command === 'logout' || command === 'clear_user_session') { fixture.signedIn = false; running = false; return null; }
      if (command === 'get_online_leagues') return { signed_in: true, enabled: false, groups: [], alias: '' };
    };
  }, { profile, completed, signedIn, licensed, usageChoice, solo, stale, legacy, offline, pomodoro, holdBoot });
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL || 'http://127.0.0.1:1443', { waitUntil: holdBoot ? 'domcontentloaded' : 'networkidle' });
  if (holdBoot) {
    await page.waitForFunction(() => Boolean(window.usageFixture.releaseBoot));
    assert.equal(await page.locator('#mainApp').isVisible(), false, 'Personal UI stays hidden while native profile initialization is pending.');
    assert.equal(await page.locator('#usageProfileLoading').isVisible(), true);
    await page.evaluate(() => window.usageFixture.releaseBoot());
  }
  await page.waitForFunction(() => document.body.dataset.usageProfile !== 'pending');
  await page.locator('#mainApp').waitFor({ state: 'visible' });
  await page.waitForFunction(() => { const label = document.getElementById('timerKindLabel')?.textContent || ''; return document.body.dataset.usageProfile.startsWith('team') ? /Recorded on this device today|Registrado hoy en este dispositivo/.test(label) : label.trim().length > 0; });
  await page.waitForTimeout(50);
  return { page, errors };
}

async function simpleRecorder(page, locale = 'en-GB') {
  // Setup completion also refreshes the clock. Wait for the rendered team label
  // rather than sampling an intermediate state on slower Windows CI runners.
  await page.locator('#timerKindLabel').filter({ hasText: locale.startsWith('es') ? /Registrado hoy en este dispositivo/ : /Recorded on this device today/ }).waitFor({ state: 'visible' });
  assert.equal(await page.locator('#navOnline').isVisible(), false);
  assert.equal(await page.locator('#navSummary').isVisible(), false);
  assert.equal(await page.locator('#navCloudInsights').isVisible(), false, await page.locator('#navCloudInsights').evaluate(element => JSON.stringify({ html: element.outerHTML, profile: document.body.dataset.usageProfile, display: getComputedStyle(element).display, css: [...document.styleSheets].flatMap(sheet => {try {return [...sheet.cssRules].filter(rule => rule.selectorText && element.matches(rule.selectorText)).map(rule => ({selector:rule.selectorText,display:rule.style.display,priority:rule.style.getPropertyPriority('display')}));} catch {return [];}}) }))); 
  assert.equal(await page.locator('#timerModeSelect').isVisible(), false);
  assert.equal(await page.locator('#timerGoalProgress').isVisible(), false);
  assert.equal(await page.locator('#sessionPlanner').isVisible(), false);
  assert.equal(await page.locator('#todayTotalFocusStart').isVisible(), false);
  assert.equal(await page.locator('#stopTimerBtn').isVisible(), false);
  assert.equal(await page.locator('#playTimerBtn').isVisible(), true, await page.evaluate(() => { let element = document.getElementById('playTimerBtn'); const ancestors = []; while (element) { const css = getComputedStyle(element); const rect = element.getBoundingClientRect(); ancestors.push({ name: element.id || element.className || element.tagName, display: css.display, visibility: css.visibility, width: rect.width, height: rect.height, hidden: element.hidden, personal: element.hasAttribute('data-personal-surface') }); element = element.parentElement; } return JSON.stringify({ profile: document.body.dataset.usageProfile, ancestors }); }));
  assert.match(await page.locator('#timerKindLabel').innerText(), locale.startsWith('es') ? /Registrado hoy en este dispositivo/ : /Recorded on this device today/);
  assert.equal(await page.locator('#timerDisplay').innerText(), '05:45:00', 'Saved Pomodoro must not replace the actual device clock in team mode.');
  assert.equal(await page.locator('.bottom-nav .nav-item:visible').count(), 2);
  assert.equal(await page.locator('.bottom-nav .nav-label:visible').count(), 2);
  const dock = await page.locator('.bottom-nav').boundingBox();
  const targets = await page.locator('.bottom-nav .nav-item:visible').evaluateAll(elements => elements.map(element => {
    const box = element.getBoundingClientRect();
    return { id: element.id, x: box.x, y: box.y, width: box.width, height: box.height };
  }));
  assert.ok(dock && dock.height < 90, 'The two team destinations must stay in one compact row.');
  assert.equal(targets.map(target => target.id).join(','), 'navToday,navProfile');
  assert.ok(Math.abs(targets[0].y - targets[1].y) < 1 && Math.abs(targets[0].width - targets[1].width) < 1);
  for (const target of targets) assert.ok(target.width >= dock.width * 0.4, `Each team destination must occupy half the dock rather than a slot in the Individual five-column grid: ${JSON.stringify({ dock, target })}`);
  for (const label of await page.locator('.bottom-nav .nav-label:visible').all()) {
    const geometry = await label.evaluate(element => ({ width: element.getBoundingClientRect().width, height: element.getBoundingClientRect().height, clip: getComputedStyle(element).clipPath }));
    assert.ok(geometry.width > 20 && geometry.height > 10 && geometry.clip === 'none', `Navigation labels must be visually readable, beyond DOM visibility: ${JSON.stringify(geometry)}`);
  }
  assert.equal(await page.locator('.today-preferences').isVisible(), false);
}

async function clearToasts(page) { await page.waitForTimeout(4600); }

async function unobstructedTeamDock(page) {
  const dock = await page.locator('.bottom-nav').boundingBox();
  for (const toast of await page.locator('.toast-container .toast:visible').all()) {
    const box = await toast.boundingBox();
    assert.ok(box && box.y + box.height <= dock.y - 8, 'Recording feedback must stay above the team navigation.');
  }
  for (const target of await page.locator('.bottom-nav .nav-item:visible').all()) {
    assert.equal(await target.evaluate(element => {
      const box = element.getBoundingClientRect();
      return document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2)?.closest('.nav-item') === element;
    }), true, 'Both team destinations remain clickable while feedback is visible.');
  }
}

try {
  if (scenario('team-first')) {
    const { page, errors } = await pageFor({ completed: false, signedIn: false });
    await page.locator('#usageChoiceOverlay.visible').waitFor();
    assert.match(await page.locator('#chooseTeamUsage').innerText(), /For my team/);
    assert.equal(await page.locator('#onboardingOverlay').isVisible(), false);
    assert.equal(await page.locator('#mainApp').evaluate(element => element.inert), true);
    await page.locator('#chooseTeamUsage').click();
    await page.locator('#loginScreen.visible.team-login').waitFor();
    assert.equal(await page.locator('.login-splash-hero').isVisible(), false);
    await page.locator('#toggleManualLogin').click(); await page.locator('#manualCodeInput').fill('fictional-ui-token'); await page.locator('#manualLoginBtn').click();
    await page.locator('#onboardingOverlay.visible.team-setup').waitFor();
    assert.match(await page.locator('#onboardingBody').innerText(), /Acme Diseño/);
    assert.equal(await page.locator('#onboardingStepLabel').isVisible(), false);
    assert.equal(await page.locator('#teamSetupShare').isChecked(), false);
    assert.equal(await page.evaluate(() => window.testCalls.filter(call => call.command === 'start_monitoring').length), 0);
    await page.locator('#teamSetupPrivacy').click(); await page.locator('#privacyNoticeOverlay.visible').waitFor();
    await page.keyboard.press('Tab'); await page.keyboard.press('Shift+Tab');
    assert.equal(await page.evaluate(() => document.getElementById('privacyNoticeOverlay').contains(document.activeElement)), true);
    await page.keyboard.press('Escape');
    await page.locator('#privacyNoticeOverlay').waitFor({state:'hidden'});
    await page.waitForFunction(()=>document.activeElement.id==='teamSetupPrivacy',undefined,{timeout:2000});
    assert.equal(await page.evaluate(() => document.activeElement.id), 'teamSetupPrivacy');
    await page.locator('#onboardingContinueBtn').click(); await page.locator('#onboardingOverlay').waitFor({ state: 'hidden' });
    assert.equal(await page.evaluate(() => window.usageFixture.privacy.monitoringNoticeAcknowledged), true);
    assert.equal(await page.evaluate(() => window.usageFixture.privacy.cloudSyncEnabled), false);
    assert.equal(await page.evaluate(() => window.testCalls.filter(call => call.command === 'start_monitoring').length), 0);
    await simpleRecorder(page); await clearToasts(page); await page.screenshot({ path: resolve(output, 'employee-first-run.png') });
    results.push({ scenario: 'team-first setup, explicit local acknowledgement, optional sharing off, nested privacy focus', passed: true });
    assert.deepEqual(errors, []); await page.close();
  }
  if (scenario('transitions')) {
    const { page, errors } = await pageFor({ solo: true, pomodoro: true, dark: true, locale: 'es-ES' });
    await simpleRecorder(page, 'es-ES');
    await page.locator('#playTimerBtn').click(); await page.locator('#playTimerLabel').filter({ hasText: 'Detener registro' }).waitFor();
    await page.clock.runFor(2500);
    const announcement = await page.locator('#trackingStateAnnouncement').innerText();
    const recorded = await page.locator('#timerDisplay').innerText(); await page.clock.runFor(2500);
    assert.equal(await page.locator('#trackingStateAnnouncement').innerText(), announcement, 'No per-second live announcements.');
    assert.notEqual(await page.locator('#timerDisplay').innerText(), recorded);
    await page.locator('#playTimerBtn').click(); await page.locator('#playTimerLabel').filter({ hasText: 'Continuar registro' }).waitFor();
    const stopped = await page.locator('#timerDisplay').innerText(); await page.clock.runFor(2500); assert.equal(await page.locator('#timerDisplay').innerText(), stopped);
    await page.locator('#teamTodayPrivacy').click();
    assert.equal(await page.locator('#cloudSyncToggle').isVisible(), true); assert.equal(await page.locator('#cloudAiToggle').isVisible(), false);
    assert.equal(await page.locator('#excludedAppsInput').isVisible(), true); assert.equal(await page.locator('#exportPersonalDataBtn').isVisible(), true);
    assert.equal(await page.locator('#mcpConnectionCard').isVisible(), false); assert.equal(await page.locator('#focusAlertsToggle').isVisible(), false);
    await page.locator('#cloudSyncToggle').check(); await page.locator('#teamTodaySharing').filter({ hasText: 'Compartir activado' }).waitFor({ state: 'attached' });
    await page.evaluate(() => window.testEmit('local-agent-digest', [{ title: 'Fictional intrusive reminder', body: 'not shown' }]));
    assert.doesNotMatch(await page.locator('body').innerText(), /Fictional intrusive reminder/);
    await page.locator('#teamLocalHistory summary').click(); await page.locator('#teamLocalHistoryRows li').first().waitFor();
    await clearToasts(page); await page.screenshot({ path: resolve(output, 'employee-settings-es-dark.png') });
    await page.locator('[data-usage-choice="individual"]').click(); await page.waitForFunction(() => document.body.dataset.usageProfile === 'individual');
    assert.equal(await page.locator('#navOnline').isVisible(), true); assert.equal(await page.locator('#navSummary').isVisible(), true);
    await page.locator('#navToday').click(); assert.equal(await page.locator('#timerModeSelect').inputValue(), 'pomodoro');
    assert.equal(await page.locator('#timerModeSelect').isVisible(), true);
    await page.locator('#navOnline').click(); await page.locator('#tabOnline.active').waitFor();
    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('set_usage_profile', { mode: 'team', teamId: 'company-a' }));
    await page.clock.runFor(61000);
    await page.waitForFunction(() => document.body.dataset.usageProfile === 'team_employee');
    assert.equal(await page.locator('#tabToday.active').count(), 1, 'Forbidden active Online tab is rescued after a native profile change.');
    assert.equal(await page.evaluate(() => document.activeElement.id), 'navToday');
    assert.equal(await page.locator('#tabOnline').evaluate(element => element.inert), true);
    await page.locator('#navProfile').click();
    assert.equal(await page.locator('#cloudSyncToggle').isChecked(), false);
    await page.evaluate(() => { window.usageFixture.failProfile = true; });
    await page.locator('[data-usage-choice="individual"]').click();
    await page.locator('#usageProfileStatus').filter({ hasText: 'No se ha podido verificar el acceso al perfil' }).waitFor();
    assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'team_employee');
    assert.equal(await page.locator('[data-usage-choice="individual"]').isEnabled(), true, 'A failed profile change is recoverable.');
    await page.evaluate(() => { window.usageFixture.failProfile = false; });
    assert.equal(await page.locator('#usageTeamSelect option[value="company-b"]').count(), 0, 'PM workspaces are managed on the web, not selected as a recorder.');
    await page.locator('#usageTeamSelect').selectOption('company-c'); await page.locator('#usageCurrentProfile').filter({ hasText: 'Gamma Design' }).waitFor();
    assert.equal(await page.locator('#usageManagerDashboard').isVisible(), true);
    await page.locator('#usageTeamSelect').selectOption('company-a'); await page.waitForFunction(() => document.body.dataset.usageProfile === 'team_employee');
    assert.equal(await page.locator('[data-usage-choice="team_manager"]').count(), 0);
    await page.locator('#navToday').click(); await page.locator('#playTimerBtn').click(); await page.locator('#playTimerLabel').filter({ hasText: 'Detener registro' }).waitFor();
    await page.locator('#navProfile').click(); await page.locator('#logoutBtn').click(); await page.waitForFunction(() => document.body.dataset.usageProfile === 'individual');
    assert.equal(await page.evaluate(() => window.usageFixture.signedIn), false);
    const calls = await page.evaluate(() => window.testCalls.map(call => call.command));
    const logoutIndex = calls.lastIndexOf('logout'); assert.ok(calls.lastIndexOf('stop_monitoring', logoutIndex) < logoutIndex && calls.lastIndexOf('stop_monitoring', logoutIndex) >= 0);
    assert.deepEqual(errors, []);
    results.push({ scenario: 'employee Stop/Continue, real clock, Spanish dark settings, local history, individual restoration, team change, manager link and stopped logout', passed: true });
    await page.close();
  }
  if (scenario('stale-cache')) {
    const { page, errors } = await pageFor({ stale: true, holdBoot: true });
    assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'individual');
    assert.equal(await page.locator('#navOnline').isVisible(), true); await page.locator('#navProfile').click();
    assert.equal(await page.locator('[data-usage-choice="individual"]').isEnabled(), true); assert.equal(await page.locator('[data-usage-choice="team_manager"]').count(), 0);
    assert.equal(await page.locator('#usageManagerDashboard').isVisible(), false);
    assert.match(await page.locator('#usageCurrentProfile').innerText(), /Individual/);
    assert.deepEqual(errors, []); results.push({ scenario: 'wrong-account cache grants no team or PM role; current account defaults to local Individual after native hydration', passed: true }); await page.close();
  }
  if (scenario('invite')) {
    const { page, errors } = await pageFor({ licensed: false, completed: false, signedIn: false, locale: 'es-ES' });
    await page.locator('#chooseTeamUsage').click();
    await page.locator('#loginScreen.visible.team-login').waitFor();
    await page.locator('#toggleManualLogin').click(); await page.locator('#manualCodeInput').fill('fictional-free-ui-token'); await page.locator('#manualLoginBtn').click();
    await page.locator('#tabProfile.active').waitFor();
    assert.equal(await page.locator('#onboardingOverlay').isVisible(), false, 'A signed-in verified Individual/free account with saved Team choice must reach invitation controls without the personal wizard.');
    assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'individual');
    assert.equal(await page.locator('#usageJoinTeam').isVisible(), true);
    assert.equal(await page.locator('#usageJoinTeam').evaluate(element => element.open), true);
    await page.locator('#teamCodeInput').fill('fictional-team-invite');
    await page.evaluate(() => { window.usageFixture.failJoin = 'This invitation was issued to another email address'; });
    await page.locator('#joinTeamBtn').click();
    await page.locator('#teamStatus').filter({ hasText: 'Esta invitación se envió a otra dirección de correo' }).waitFor();
    assert.equal(await page.locator('#joinTeamBtn').isEnabled(), true);
    assert.equal(await page.evaluate(() => window.usageFixture.licensed), false);
    await page.evaluate(() => { window.usageFixture.failJoin = null; });
    await page.locator('#teamCodeInput').fill('fictional-team-invite'); await page.locator('#joinTeamBtn').click();
    await page.locator('#onboardingOverlay.visible.team-setup').waitFor();
    assert.equal(await page.locator('#teamSetupShare').isChecked(), false);
    assert.equal(await page.evaluate(() => window.testCalls.filter(call => call.command === 'start_monitoring').length), 0);
    await page.locator('#onboardingContinueBtn').click(); await page.locator('#onboardingOverlay').waitFor({ state: 'hidden' });
    await page.clock.runFor(61000);
    const calls = await page.evaluate(() => window.testCalls.map(call => call.command));
    assert.ok(calls.includes('join_team'));
    assert.equal(calls.some(command => ['get_online_leagues', 'bip_collect_local', 'bip_cloud', 'bip_generate_local'].includes(command)), false, 'Team profiles must not invoke personal Online or content cadence commands.');
    assert.deepEqual(errors, []); results.push({ scenario: 'real verified Individual/free payload after Team-first login reaches invitation without personal wizard; team join, translated rejection/retry, explicit setup and no personal background collection', passed: true });
    await page.locator('#navProfile').click(); await page.locator('#usageProfileCard').scrollIntoViewIfNeeded();
    await clearToasts(page); await page.screenshot({ path: resolve(output, 'employee-profile-es.png') }); await page.close();
  }
  if (scenario('claim')) {
    const { page, errors } = await pageFor({ licensed: false, completed: false, usageChoice: 'team' });
    await page.locator('#onboardingSkipCalendarBtn').click(); await page.locator('#navProfile').click();
    await page.evaluate(() => { window.usageFixture.preferences.onboardingCompleted = false; });
    await page.locator('#profileLicenseCode').fill('FS-FICTIONAL-CODE'); await page.locator('#claimLicenseBtn').click();
    await page.locator('#onboardingOverlay.visible.team-setup').waitFor();
    assert.equal(await page.evaluate(() => window.testCalls.filter(call => call.command === 'claim_license_code').length), 1);
    assert.equal(await page.evaluate(() => window.testCalls.some(call => call.command === 'start_monitoring')), false);
    assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'team_employee');
    assert.equal(await page.locator('#teamSetupShare').isChecked(), false);
    assert.deepEqual(errors, []); results.push({ scenario: 'canonical native license claim resolves employee profile and explicit setup without automatic recording or sharing', passed: true }); await page.close();
  }
  if (scenario('individual')) {
    const { page, errors } = await pageFor({ completed: false, signedIn: false });
    await page.locator('#chooseIndividualUsage').click(); await page.locator('#onboardingOverlay.visible:not(.team-setup)').waitFor();
    assert.match(await page.locator('#onboardingStepLabel').innerText(), /1.*6/);
    assert.equal(await page.locator('#onboardingBody input').count() > 0, true);
    assert.equal(await page.locator('#teamSetupShare').count(), 0);
    assert.deepEqual(errors, []); results.push({ scenario: 'individual first choice preserves the existing six-step personal setup', passed: true }); await page.close();
  }
  if (scenario('navigation')) {
    for (const config of [
      { name: 'employee-compact', width: 340, height: 400, locale: 'es-ES' },
      { name: 'employee-default', width: 370, height: 700, locale: 'es-ES' },
      { name: 'employee-dark', width: 370, height: 700, locale: 'en-GB', dark: true },
      { name: 'employee-wide', width: 900, height: 1000, locale: 'es-ES' },
    ]) {
      const { page, errors } = await pageFor(config);
      await page.setViewportSize({ width: config.width, height: config.height });
      await simpleRecorder(page, config.locale);
      if (config.name === 'employee-default') await page.screenshot({ path: resolve(output, 'employee-navigation.png') });
      await page.locator('#playTimerBtn').click();
      await page.locator('#playTimerLabel').filter({ hasText: config.locale.startsWith('es') ? 'Detener registro' : 'Stop recording' }).waitFor();
      assert.doesNotMatch(await page.locator('.toast-container').innerText(), /Local AI Server Ready|Servidor local de IA listo/);
      await unobstructedTeamDock(page);
      await page.locator('#navProfile').click(); await page.locator('#tabProfile.active').waitFor();
      await page.locator('#navToday').focus(); await page.keyboard.press('Enter'); await page.locator('#tabToday.active').waitFor();
      await page.locator('#playTimerBtn').click();
      await page.locator('#playTimerLabel').filter({ hasText: config.locale.startsWith('es') ? 'Continuar registro' : 'Continue recording' }).waitFor();
      await page.locator('.toast-container .toast').first().waitFor();
      await page.clock.runFor(400);
      await unobstructedTeamDock(page);
      if (config.name === 'employee-default') await page.screenshot({ path: resolve(output, 'employee-navigation-feedback.png') });
      assert.deepEqual(errors, []); await page.close();
    }
    const { page, errors } = await pageFor({ profile: 'individual', solo: true, locale: 'es-ES' });
    assert.equal(await page.locator('.bottom-nav .nav-item:visible').count(), 5);
    const dock = await page.locator('.bottom-nav').boundingBox(), clock = await page.locator('#navToday').boundingBox();
    assert.ok(Math.abs(clock.x + clock.width / 2 - (dock.x + dock.width / 2)) < 1, 'The Individual clock remains centered among its five destinations.');
    assert.deepEqual(errors, []); await page.close();
    results.push({ scenario: 'equal-width team destinations in one row at minimum, default and wide sizes; Spanish/English and light/dark; no technical server-ready notification; recording feedback does not cover pointer or keyboard navigation; Individual keeps five centered destinations', passed: true });
  }
  if (scenario('free-individual')) {
    const { page, errors } = await pageFor({ licensed: false, completed: false, usageChoice: 'team' });
    await page.locator('#onboardingOverlay.visible:not(.team-setup)').waitFor();
    assert.equal(await page.evaluate(() => localStorage.getItem('flowsight_usage_onboarding_choice')), 'individual');
    assert.equal(await page.evaluate(() => window.testCalls.some(call => call.command === 'set_usage_profile')), false, 'Explicit same-profile choice changes the setup preference without inventing or requesting a different licensed role.');
    assert.match(await page.locator('#onboardingStepLabel').innerText(), /1.*6/);
    assert.deepEqual(errors, []); results.push({ scenario: 'verified free account can explicitly choose its existing Individual profile and open personal setup after previously choosing Team', passed: true }); await page.close();
  }
  if (scenario('individual-defaults')) {
    for (const config of [{ signedIn: false, usageChoice: 'team', completed: false }, { profile: 'individual', legacy: true }, { licensed: false }]) {
      const { page, errors } = await pageFor(config);
      assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'individual');
      assert.equal(await page.locator('#navOnline').isVisible(), true); assert.equal(await page.locator('#navSummary').isVisible(), true);
      assert.equal(await page.locator('#loginScreen.visible.team-login').count(), 0);
      assert.deepEqual(errors, []); await page.close();
    }
    const { page, errors } = await pageFor({ offline: true, pomodoro: true });
    await simpleRecorder(page); assert.equal(await page.locator('#focusAlertsToggle').isVisible(), false);
    assert.deepEqual(errors, []); await page.close();
    results.push({ scenario: 'disconnected, signed-in free and legacy paid Individual defaults; stored Team choice does not force login; known offline employee retains simple recorder', passed: true });
  }
  if (scenario('pm-web')) {
    const { page, errors } = await pageFor({ profile: 'individual', solo: true, locale: 'es-ES' });
    await page.locator('#navProfile').click();
    assert.equal(await page.locator('[data-usage-choice="team_manager"]').count(), 0);
    assert.equal(await page.locator('#usageManagerDashboard').isVisible(), true);
    assert.equal(await page.locator('#usageManagerDashboard').getAttribute('href'), 'https://app.flowsight.site/dashboard/team');
    assert.equal(await page.locator('body').getAttribute('data-usage-profile'), 'individual');
    await page.locator('#usageProfileCard').screenshot({ path: resolve(output, 'pm-web-profiles.png') });
    assert.deepEqual(errors, []); await page.close();
    results.push({ scenario: 'PM management link opens the existing web dashboard and no PM recorder mode exists', passed: true });
  }
  await writeFile(resolve(output, 'renderer-e2e.json'), JSON.stringify({ evidence: 'Production renderer with fictional native IPC, no real account or native capture', results }, null, 2));
  console.log(JSON.stringify(results, null, 2));
} catch(error) {
  if(currentPage&&!currentPage.isClosed()) {
    await currentPage.screenshot({path:resolve(output,'failure-simulated-native.png'),timeout:10000}).catch(()=>{});
    console.error('Profile failure focus:',await currentPage.evaluate(()=>({
      activeId:document.activeElement?.id,activeTag:document.activeElement?.tagName,
      inertAncestor:document.activeElement?.closest('[inert]')?.id,
      privacyVisible:document.getElementById('privacyNoticeOverlay')?.classList.contains('visible'),
      setupVisible:document.getElementById('onboardingOverlay')?.classList.contains('visible'),
    })).catch(()=>null));
  }
  throw error;
} finally { await browser.close(); }
