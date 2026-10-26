// Production renderer with fictional tracking records and isolated Tauri IPC.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright';

const output = resolve(process.env.FLOWSIGHT_EVIDENCE_DIR || '.impeccable/review/daily-flow-week-visible');
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
const evidence = [];
try {
  for (const locale of ['en-GB', 'es-ES']) {
    const page = await browser.newPage({ locale, timezoneId: 'Europe/Madrid', viewport: { width: 900, height: 1000 }, colorScheme: 'light', reducedMotion: 'reduce' });
    await page.clock.install({ time: new Date('2026-10-03T12:00:00+02:00') });
    await page.clock.pauseAt(new Date('2026-10-03T12:00:00+02:00'));
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(({ locale }) => {
      const saved = JSON.parse(sessionStorage.getItem('daily_flow_fixture') || 'null');
      window.flowFixture = saved || { date: '2026-10-03', total_seconds: 600, completed_dates: ['2026-09-29', '2026-10-01', '2026-10-02'], total_wins: 5, streak: 13 };
      window.flowFailure = false;
      window.testCalls = [];
      let running = true, nextId = 1;
      const callbacks = new Map();
      const clock = () => ({ date: window.flowFixture.date, total_seconds: window.flowFixture.total_seconds, total_milliseconds: window.flowFixture.total_seconds * 1000, is_running: running });
      const responses = {
        initialize_agent: null, get_config: { captureInterval: 60000, dailyGoalHours: 6 },
        get_auth_session: null, get_current_user: null,
        get_entitlements: { plan: 'free', status: 'active', can_integrations: false, can_cloud_ai: false, can_sync: false, team_ids: [] },
        get_privacy_settings: { monitoringNoticeAcknowledged: true, noticeVersion: '2026-08-23', cloudSyncEnabled: false, cloudAiEnabled: false, storeWindowTitles: false, excludedApplications: [], retentionDays: 30 },
        get_user_preferences: { onboardingCompleted: true, dailyGoalHours: 6, workRoles: [], workActivities: [], improvementGoals: [] },
        get_analytics_consent: { decided: true, consented: false }, check_installation_health: { healthy: true }, check_local_server: { online: true },
        get_week_summary: { days: [] }, get_local_agent_data: { events: [], preferences: {}, tasks: [] },
        get_calendar_companion_status: { googleConnected: false, microsoftConnected: false, googleAvailable: false, current: null },
        get_browser_pairing: { connected: false }, get_desktop_preferences: { focusAlertsEnabled: false, contextualFocusAlertsEnabled: false, promptDecided: true },
        get_language_preference: { preference: 'system', systemLanguage: locale === 'es-ES' ? 'es' : 'en', persisted: true },
        get_coach_chat_messages: [], get_coach_chat_usage: { used: 0 },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
        transformCallback(fn) { const id = nextId++; callbacks.set(id, fn); return id; },
        unregisterCallback(id) { callbacks.delete(id); }, convertFileSrc(value) { return value; },
        async invoke(command, args = {}) {
          window.testCalls.push({ command, args });
          if (command === 'get_daily_flow_progress') {
            if (window.flowFailure) throw new Error('Synthetic tracking store unavailable');
            sessionStorage.setItem('daily_flow_fixture', JSON.stringify(window.flowFixture));
            return structuredClone(window.flowFixture);
          }
          if (command === 'get_tracking_clock') return clock();
          if (command === 'get_status') return { isRunning: running };
          if (command === 'get_today_history') return { date: window.flowFixture.date, total_seconds: window.flowFixture.total_seconds, tracking: clock(), entries: [], category_breakdown: [], ticket_breakdown: [], focus: { deep_focus_seconds: 0, deep_threshold_seconds: 1500, distraction_events: 0, browsing_distraction_min_seconds: 120, sensor_grace_seconds: 120, themes: [] } };
          if (command === 'start_monitoring' || command === 'stop_monitoring') { running = command === 'start_monitoring'; return true; }
          if (command === 'plugin:event|listen') return args.handler;
          if (command.startsWith('plugin:window|')) return command.endsWith('is_maximized') ? false : null;
          if (command.startsWith('plugin:')) return null;
          return structuredClone(responses[command] ?? null);
        },
      };
    }, { locale });
    await page.goto(process.env.FLOWSIGHT_RENDERER_URL || 'http://127.0.0.1:1433', { waitUntil: 'networkidle' });
    // Static evidence measures the settled theme; the paused clock can otherwise
    // leave descendant color transitions between the old and new appearances.
    await page.addStyleTag({ content: '*, *::before, *::after { transition: none !important; }' });
    assert.equal(await page.locator('#tabToday #dailyFlow').count(), 0);
    assert.equal(await page.locator('#timerStreak').count(), 0);
    await page.locator('#navSummary').click();
    await page.locator('#summaryBody #dailyFlow').waitFor();
    assert.equal(await page.locator('#dailyFlow').evaluate(node => node.open), false);
    assert.equal(await page.locator('.daily-flow-mission').isVisible(), false);
    assert.equal(await page.locator('#dailyFlowToggle .daily-flow-week').isVisible(), true);
    assert.equal(await page.locator('#dailyFlowToggle .daily-flow-day').count(), 7);
    assert.equal(await page.locator('#dailyFlowToggle .daily-flow-day.is-done').count(), 3);
    assert.match(await page.locator('#dailyFlowDate').innerText(), locale === 'es-ES' ? /Hoy/ : /Today/);
    await page.locator('#dailyFlowToggle').focus();
    await page.keyboard.press('Enter');
    await page.locator('.daily-flow-mission').waitFor();
    assert.match(await page.locator('.daily-flow-count').innerText(), /5/);
    assert.equal(await page.locator('#streakText').innerText(), locale === 'es-ES' ? '13 días' : '13 days');
    assert.equal(await page.locator('.daily-flow-day.is-done').count(), 3);
    assert.equal(await page.locator('.daily-flow-track').getAttribute('aria-valuenow'), '66');
    assert.equal(await page.locator('.daily-flow-milestones li.is-earned').count(), 2);
    await page.locator('#dailyFlowToggle').evaluate(node => node.blur());
    await page.mouse.move(0, 0);

    // One batched visual pass across desktop / compact windows and both themes.
    for (const colorScheme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme });
      // A non-matching stylesheet can load asynchronously when activated. Wait
      // for its rules, then render once before measuring and taking the PNG.
      await page.waitForFunction(scheme => {
        const links = ['themeDarkBase', 'themeDarkMobile'].map(id => document.getElementById(id));
        return document.documentElement.dataset.theme === scheme && (scheme !== 'dark' || links.every(link => link.media === 'all' && link.sheet));
      }, colorScheme);
      await page.waitForFunction(scheme => {
        const background = getComputedStyle(document.querySelector('.daily-flow-detail')).backgroundColor;
        const foreground = getComputedStyle(document.querySelector('.daily-flow-heading h2')).color;
        return scheme === 'dark'
          ? background === 'rgb(25, 33, 41)' && foreground === 'rgb(227, 235, 237)'
          : background === 'rgb(255, 255, 255)' && foreground === 'rgb(15, 23, 41)';
      }, colorScheme);
      // Playwright's clock is paused; advance it to settle scheduled frames.
      await page.clock.runFor(50);
      for (const width of [900, 370, 320]) {
        await page.setViewportSize({ width, height: width === 900 ? 1100 : 950 });
        if (width === 900) await page.locator('.tab-content').evaluate(element => { element.scrollTop = 0; });
        else await page.locator('#dailyFlow').scrollIntoViewIfNeeded();
        await page.clock.runFor(50);
        const layout = await page.locator('#dailyFlow').evaluate(root => {
          const rect = root.getBoundingClientRect();
          const background = getComputedStyle(root).backgroundColor;
          return { width: root.clientWidth, scroll: root.scrollWidth, rect: { left: rect.left, right: rect.right }, viewport: innerWidth, background,
            texts: [...root.querySelectorAll('h2, h3, strong, time, .daily-flow-day > span:first-child, li > span, p')].map(node => ({ text: node.textContent, color: getComputedStyle(node).color, background: getComputedStyle(node.closest('.daily-flow-detail') || document.body).backgroundColor })) };
        });
        assert.ok(layout.scroll <= layout.width + 1 && layout.rect.left >= 0 && layout.rect.right <= layout.viewport, JSON.stringify(layout));
        const luminance = color => color.match(/[0-9.]+/g).slice(0, 3).map(Number).map(value => value / 255).map(value => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4).reduce((sum, value, index) => sum + value * [0.2126, 0.7152, 0.0722][index], 0);
        for (const text of layout.texts) {
          const a = luminance(text.color), b = luminance(text.background);
          text.contrast = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
          assert.ok(text.contrast >= 4.5, JSON.stringify(text));
        }
        await page.screenshot({ path: resolve(output, `daily-flow-expanded-${locale}-${colorScheme}-${width}.png`) });
        await page.locator('#dailyFlowToggle').click();
        assert.equal(await page.locator('.daily-flow-mission').isVisible(), false);
        assert.equal(await page.locator('.daily-flow-week').isVisible(), true);
        assert.equal(await page.locator('.daily-flow-week').count(), 1, 'Week appears once, including when expanded.');
        assert.equal(await page.locator('.daily-flow-day-mark').count(), 7);
        await page.locator('.tab-content').evaluate(element => { element.scrollTop = 0; });
        await page.locator('#dailyFlowToggle').evaluate(node => node.blur());
        await page.mouse.move(0, 0);
        await page.clock.runFor(50);
        await page.screenshot({ path: resolve(output, `daily-flow-collapsed-${locale}-${colorScheme}-${width}.png`) });
        assert.ok(await page.locator('#dailyFlow').evaluate(root => root.getBoundingClientRect().height <= 110), 'Collapsed date/streak and weekly dots stay compact.');
        await page.locator('#dailyFlowToggle').click();
        await page.locator('#dailyFlowToggle').evaluate(node => node.blur());
        await page.mouse.move(0, 0);
        await page.clock.runFor(50);
        evidence.push({ locale, colorScheme, width, layout });
      }
    }
    await page.setViewportSize({ width: 900, height: 1100 });
    // Report refreshes and tab navigation preserve the disclosure node and state.
    const panelBefore = await page.locator('#dailyFlow').elementHandle();
    await page.locator('#navToday').click();
    await page.locator('#navSummary').click();
    assert.equal(await page.locator('#dailyFlow').evaluate(node => node.open), true);
    assert.equal(await page.locator('#dailyFlow').evaluate((node, previous) => node === previous, panelBefore), true);
    await page.locator('#navToday').click();
    assert.equal(await page.locator('#playTimerBtn').getAttribute('aria-label'), locale === 'es-ES' ? 'Pausar seguimiento' : 'Pause tracking');
    await page.evaluate(() => { window.flowFixture.total_seconds = 900; window.flowFixture.total_wins = 6; window.flowFixture.completed_dates.push('2026-10-03'); });
    await page.clock.runFor(16000);
    await page.locator('#navSummary').click();
    await page.locator('.daily-flow--complete').waitFor();
    assert.equal(await page.locator('#dailyFlow').evaluate(node => node.open), true);
    assert.match(await page.locator('#dailyFlowAnnouncement').textContent(), locale === 'es-ES' ? /Logro diario conseguido/ : /Daily win earned/);
    assert.equal(await page.locator('.daily-flow-day.is-done').count(), 4);
    await page.locator('#dailyFlow').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, `daily-flow-earned-${locale}.png`) });
    await page.locator('#navToday').click();
    await page.locator('#playTimerBtn').click();
    const winsBefore = await page.locator('.daily-flow-day.is-done').count();
    await page.clock.runFor(60000);
    assert.equal(await page.locator('.daily-flow-day.is-done').count(), winsBefore, 'Paused time must not add wins.');

    await page.reload({ waitUntil: 'networkidle' });
    await page.locator('#navSummary').click();
    await page.locator('.daily-flow--complete').waitFor();
    assert.equal(await page.locator('#dailyFlow').evaluate(node => node.open), false, 'Fresh loads show date, streak and weekly dots.');
    assert.equal(await page.locator('.daily-flow-week').isVisible(), true, 'Fresh loads retain weekly dots.');
    assert.equal(await page.locator('#dailyFlowAnnouncement').textContent(), '', 'Reload is quiet and preserves the completed state.');
    await page.locator('#navProfile').click();
    await page.locator('#dailyFlowVisible').uncheck();
    await page.locator('#navSummary').click();
    assert.equal(await page.locator('#dailyFlow').isVisible(), false);
    await page.reload({ waitUntil: 'networkidle' });
    assert.equal(await page.locator('#dailyFlow').isVisible(), false, 'Hide preference persists.');
    await page.locator('#navProfile').click();
    await page.locator('#dailyFlowVisible').check();
    await page.locator('#navSummary').click();
    await page.locator('#dailyFlowToggle').click();
    await page.locator('.daily-flow-mission').waitFor();

    await page.evaluate(() => { window.flowFailure = true; });
    await page.locator('#navProfile').click();
    await page.locator('#navSummary').click();
    await page.locator('[data-flow-retry]').waitFor();
    await page.evaluate(() => { window.flowFailure = false; });
    await page.locator('[data-flow-retry]').click();
    await page.locator('.daily-flow-mission').waitFor();
    assert.equal(await page.locator('#dailyFlowAnnouncement').textContent(), '', 'Recovery never repeats the celebration.');

    // Local day rollover clears today's progress but preserves the previous streak.
    await page.clock.setSystemTime(new Date('2026-10-04T00:01:00+02:00'));
    await page.evaluate(() => { window.flowFixture.date = '2026-10-04'; window.flowFixture.total_seconds = 0; });
    await page.locator('#navProfile').click();
    await page.locator('#navSummary').click();
    await page.locator('.daily-flow-count').filter({ hasText: '15' }).waitFor();
    assert.equal(await page.locator('#streakText').innerText(), locale === 'es-ES' ? '13 días' : '13 days');
    assert.equal(await page.locator('.daily-flow-track').getAttribute('aria-valuenow'), '0');
    // The same Insights slot also exists before any activity is recorded.
    await page.locator('#dailyFlowToggle').click();
    await page.reload({ waitUntil: 'networkidle' });
    await page.locator('#navSummary').click();
    await page.locator('#summaryBody #dailyFlow').waitFor();
    assert.equal(await page.locator('#dailyFlow').count(), 1);
    assert.equal(await page.locator('#dailyFlow').evaluate(node => node.open), false);
    assert.equal(await page.locator('.summary-main-card').count(), 0);
    assert.match(await page.locator('#summaryBody .empty-state').innerText(), locale === 'es-ES' ? /no hay actividad/i : /No activity/);
    await page.locator('#dailyFlowToggle').click();
    await page.locator('.daily-flow-mission').waitFor();
    await page.clock.runFor(50);
    await page.screenshot({ path: resolve(output, `daily-flow-empty-${locale}.png`) });
    assert.deepEqual(errors, []);
    evidence.push({ locale, tests: { insightsPlacement: true, collapsedByDefault: true, collapsedWeekVisible: true, singleWeekStrip: true, keyboardDisclosure: true, openStateSurvivesRefresh: true, emptyHistory: true, freeWithoutAccount: true, savedWins: true, liveWin: true, reloadQuiet: true, pause: true, hidePersists: true, retry: true, localMidnight: true }, pageErrors: errors });
    await page.close();
  }
  await writeFile(resolve(output, 'verification.json'), JSON.stringify(evidence, null, 2));
  console.log('Daily Flow in Insights: collapsed/expanded in both languages and themes at 900/370/320px; keyboard, refresh persistence, wins, pause, reload, hide, retry and midnight passed.');
} finally { await browser.close(); }
