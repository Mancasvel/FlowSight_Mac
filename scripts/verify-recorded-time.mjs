// Actual production UI with fictional activity and isolated native IPC.
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { chromium } from 'playwright';

const output = resolve('.impeccable/review/recorded-time');
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true });
const evidence = [];
try {
  for (const locale of ['es-ES', 'en-GB']) {
    const page = await browser.newPage({ locale, timezoneId: 'Europe/Madrid', viewport: { width: 370, height: 700 }, colorScheme: 'dark', reducedMotion: 'reduce' });
    await page.clock.install({ time: new Date('2026-10-01T17:00:00+02:00') });
    await page.clock.pauseAt(new Date('2026-10-01T17:00:00+02:00'));
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(({ locale }) => {
      window.testCalls = [];
      window.recordedHistory = {
        date: '2026-10-01', total_seconds: 20700,
        entries: [{ time: '2026-10-01 16:59:00', duration_seconds: 20700, category: 'Research', description: 'Synthetic ADDA study', ticket: null }],
        category_breakdown: [{ category: 'Research', total_seconds: 20700 }], ticket_breakdown: [],
        focus: { deep_focus_seconds: 13380, distraction_events: 0, hourly_deep_focus: [], sensor_grace_seconds: 120, browsing_distraction_min_seconds: 120, themes: [] },
      };
      window.recordedHistory.entries.push(...Array.from({ length: 30 }, (_, index) => ({
        time: `2026-10-01 ${String(16 - Math.floor(index / 6)).padStart(2, '0')}:${String(55 - index % 6 * 5).padStart(2, '0')}:00`,
        duration_seconds: 0, category: 'Research', ticket: null,
        description: `Synthetic study resource ${index}: reviewing virtual graphs, recursive types and genetic algorithms. Keep this explanation visible while a new observation arrives.`,
      })));
      window.historyFailure = false;
      // Reproduce the user's inflated checkpoint from the older renderer.
      if (!sessionStorage.getItem('synthetic_native_clock')) localStorage.setItem('flowsight_tracking_checkpoint_v1', JSON.stringify({ version: 1, mode: 'running', totalSeconds: 21571, updatedAt: Date.now(), day: '2026-10-01' }));
      const savedClock = JSON.parse(sessionStorage.getItem('synthetic_native_clock') || 'null');
      let running = savedClock?.is_running ?? true, counter = 1;
      let trackedMs = savedClock?.total_milliseconds ?? 20700000, lastTrackingTick = performance.now();
      const trackingSnapshot = () => {
        const tick = performance.now();
        if (running) trackedMs += Math.max(0, tick - lastTrackingTick);
        lastTrackingTick = tick;
        const d = new Date();
        const day = `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
        if (day !== '2026-10-01') trackedMs = 0;
        const snapshot = { date: day, total_seconds: Math.floor(trackedMs / 1000), total_milliseconds: trackedMs, is_running: running };
        sessionStorage.setItem('synthetic_native_clock', JSON.stringify(snapshot));
        return snapshot;
      };
      const callbacks = new Map(), listeners = new Map();
      const responses = {
        initialize_agent: null, get_config: { captureInterval: 60000, dailyGoalHours: 6 },
        get_auth_session: null, get_current_user: null,
        get_entitlements: { plan: 'free', status: 'active', can_integrations: false, can_cloud_ai: false, can_sync: false, team_ids: [] },
        get_privacy_settings: { monitoringNoticeAcknowledged: true, cloudSyncEnabled: false, cloudAiEnabled: false, storeWindowTitles: false, excludedApplications: [], retentionDays: 30 },
        get_user_preferences: { onboardingCompleted: true, dailyGoalHours: 6, workRoles: [], workActivities: [], improvementGoals: [] },
        get_analytics_consent: { decided: true, consented: false }, check_installation_health: { healthy: true }, check_local_server: { online: true },
        get_week_summary: { days: [] }, get_local_agent_data: { events: [], preferences: {}, tasks: [] },
        get_calendar_companion_status: { googleConnected: false, microsoftConnected: false, googleAvailable: false, current: null },
        get_browser_pairing: { connected: false }, get_desktop_preferences: { focusAlertsEnabled: false, contextualFocusAlertsEnabled: false, promptDecided: true },
        get_language_settings: { preference: 'system', systemLanguage: locale === 'es-ES' ? 'es' : 'en' },
        get_coach_chat_messages: [], get_coach_chat_usage: { used: 0 },
      };
      window.testEmit = (event, payload = {}) => {
        for (const handler of listeners.get(event) || []) callbacks.get(handler)?.({ event, id: handler, payload });
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
        transformCallback(fn) { const id = counter++; callbacks.set(id, fn); return id; },
        unregisterCallback(id) { callbacks.delete(id); }, convertFileSrc(value) { return value; },
        async invoke(command, args = {}) {
          window.testCalls.push({ command, args });
          if (command === 'get_tracking_clock') return trackingSnapshot();
          if (command === 'get_status') return { isRunning: running };
          if (command === 'get_today_history') {
            if (window.historyFailure) throw new Error('Synthetic history unavailable');
            return { ...structuredClone(window.recordedHistory), tracking: trackingSnapshot() };
          }
          if (command === 'start_monitoring' || command === 'stop_monitoring') { trackingSnapshot(); running = command === 'start_monitoring'; lastTrackingTick = performance.now(); trackingSnapshot(); return true; }
          if (command === 'plugin:event|listen') { listeners.set(args.event, [...(listeners.get(args.event) || []), args.handler]); return args.handler; }
          if (command.startsWith('plugin:window|')) return command.endsWith('is_maximized') ? false : null;
          if (command.startsWith('plugin:')) return null;
          return structuredClone(responses[command] ?? null);
        },
      };
    }, { locale });
    await page.goto(process.env.FLOWSIGHT_RENDERER_URL || 'http://127.0.0.1:1420', { waitUntil: 'networkidle' });
    await page.locator('#timerDisplay').filter({ hasText: '05:45:00' }).waitFor();
    assert.equal(await page.locator('#timerModeSelect').inputValue(), 'normal');
    assert.equal(await page.locator('#timerRecordingNote').isVisible(), false);
    assert.doesNotMatch(await page.locator('body').innerText(), /Live timer · pauses|Temporizador en directo · se detiene/);
    for (const expected of ['05:45:01', '05:45:02', '05:45:03']) {
      await page.clock.runFor(1000);
      assert.equal((await page.locator('#timerDisplay').innerText()).trim(), expected, 'Normal timer must advance every second.');
    }
    await page.clock.runFor(117000);
    assert.equal((await page.locator('#timerDisplay').innerText()).trim(), '05:47:00', 'Delayed updates must retain actual elapsed time.');
    await page.locator('#sessionPlannerToggle').evaluate(element => { if (element.getAttribute('aria-expanded') === 'true') element.click(); });
    await page.locator('.today-timer-section').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, `today-${locale}.png`) });
    await page.locator('#navSummary').click();
    await page.locator('.summary-total-time').filter({ hasText: '5 h 47 min' }).waitFor();
    assert.match(await page.locator('.summary-focus-ratio').innerText(), /65%/);
    await page.screenshot({ path: resolve(output, `insights-${locale}.png`) });
    const resource = page.locator('.timeline-item').filter({ hasText: 'Synthetic study resource 18:' });
    await resource.evaluate(element => {
      const scroller = element.closest('.tab-content');
      scroller.scrollTop += element.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 40;
    });
    const topBefore = await resource.evaluate(element => element.getBoundingClientRect().top);
    const oldHandle = await resource.elementHandle();
    await page.clock.runFor(30000);
    assert.equal(await oldHandle.evaluate(element => element.isConnected), true, 'Unchanged polling must preserve the existing report DOM.');
    await page.screenshot({ path: resolve(output, `reading-before-${locale}.png`) });
    await page.evaluate(() => {
      window.recordedHistory.entries.unshift({
        time: '2026-10-01 17:00:00', duration_seconds: 0, category: 'Research', ticket: null,
        description: 'New synthetic resource above the item currently being read.',
      });
      window.testEmit('activity-report', { id: 100 });
    });
    await page.getByText('New synthetic resource above the item currently being read.', { exact: true }).waitFor({ state: 'attached' });
    const topAfter = await resource.evaluate(element => element.getBoundingClientRect().top);
    assert.ok(Math.abs(topAfter - topBefore) <= 2, `New resources must preserve reading offset: ${topBefore} → ${topAfter}`);
    await page.screenshot({ path: resolve(output, `reading-after-${locale}.png`) });
    await page.evaluate(() => { window.historyFailure = true; window.testEmit('activity-report'); });
    await page.clock.runFor(16000);
    assert.equal(await resource.count(), 1, 'Transient refresh failure must keep the report being read.');
    assert.ok(Math.abs(await resource.evaluate(element => element.getBoundingClientRect().top) - topAfter) <= 2);
    await page.evaluate(() => { window.historyFailure = false; });
    await page.evaluate(() => {
      window.recordedHistory.total_seconds = 20760;
      window.recordedHistory.entries[0].duration_seconds = 20760;
      window.recordedHistory.category_breakdown[0].total_seconds = 20760;
      window.recordedHistory.focus.deep_focus_seconds = 13440;
      window.testEmit('activity-report', { id: 2 });
    });
    await page.clock.runFor(1000);
    const afterAnalysis = (await page.locator('#timerDisplay').textContent()).trim();
    assert.match(afterAnalysis, /^05:47:/, 'An analysis result must not add recorded duration to the running timer.');
    assert.match(await page.locator('.summary-total-time').innerText(), /5 h 47 min/);
    await page.locator('#navToday').click();
    await page.locator('#playTimerBtn').click();
    await page.locator('#todayTrackingState').filter({ hasText: locale === 'es-ES' ? 'En pausa' : 'Paused' }).waitFor();
    const pausedTime = (await page.locator('#timerDisplay').innerText()).trim();
    await page.clock.runFor(60000);
    assert.equal((await page.locator('#timerDisplay').innerText()).trim(), pausedTime, 'Pause must preserve elapsed time.');
    await page.locator('#playTimerBtn').click();
    await page.locator('#todayTrackingState').filter({ hasText: locale === 'es-ES' ? 'En directo' : 'Live' }).waitFor();
    await page.clock.runFor(1000);
    assert.notEqual((await page.locator('#timerDisplay').innerText()).trim(), pausedTime, 'Resume must advance without adding paused time.');
    await page.locator('#playTimerBtn').click();
    await page.locator('#todayTrackingState').filter({ hasText: locale === 'es-ES' ? 'En pausa' : 'Paused' }).waitFor();
    await page.locator('#timerModeSelect').selectOption('pomodoro');
    assert.equal((await page.locator('#timerDisplay').innerText()).trim(), '25:00');
    await page.locator('#pomodoroPanel summary').click();
    await page.locator('#pomodoroWork').selectOption('15');
    assert.equal((await page.locator('#timerDisplay').innerText()).trim(), '15:00');
    await page.locator('#pomodoroPanel summary').click();
    await page.locator('#playTimerBtn').click();
    await page.locator('#todayTrackingState').filter({ hasText: locale === 'es-ES' ? 'En directo' : 'Live' }).waitFor();
    await page.clock.runFor(1000);
    assert.equal((await page.locator('#timerDisplay').innerText()).trim(), '14:59');
    await page.clock.runFor(899000);
    await page.clock.runFor(1000);
    await page.locator('#todayTrackingState').filter({ hasText: locale === 'es-ES' ? 'En pausa' : 'Paused' }).waitFor();
    assert.match((await page.locator('#timerDisplay').innerText()).trim(), /^0[45]:[0-5]\d$/);
    assert.equal(await page.locator('#playTimerBtn').isDisabled(), true, 'Work must not restart during the break.');
    await page.locator('.today-timer-section').scrollIntoViewIfNeeded();
    await page.screenshot({ path: resolve(output, `pomodoro-break-${locale}.png`) });
    await page.clock.runFor(300000);
    assert.equal(await page.locator('#playTimerBtn').isDisabled(), false, 'Next work interval must wait for the user.');
    await page.locator('#timerModeSelect').selectOption('normal');
    await page.screenshot({ path: resolve(output, `normal-selector-${locale}.png`) });
    assert.equal(await page.locator('#timerModeSelect').evaluate(element => element.getBoundingClientRect().right <= document.documentElement.clientWidth), true);
    await page.clock.runFor(7000);
    for (const colorScheme of ['light', 'dark']) {
      await page.emulateMedia({ colorScheme });
      await page.locator('#timerModeSelect').selectOption('pomodoro');
      await page.locator('#pomodoroPanel summary').click();
      for (const viewport of [{width:320,height:600}, {width:370,height:700}, {width:900,height:760}]) {
        await page.setViewportSize(viewport);
        await page.locator('.today-timer-ring').evaluate(card => {
          const scroller=card.closest('.tab-content');
          scroller.scrollTop += card.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 10;
        });
        const layout = await page.locator('.today-timer-ring').evaluate(card => {
          const rect=card.getBoundingClientRect();
          const items=[...card.querySelectorAll('select')].map(node=>node.getBoundingClientRect());
          const mode=card.querySelector('#timerModeSelect').getBoundingClientRect();
          const label=card.querySelector('#timerKindLabel').getBoundingClientRect();
          return {inside: items.every(item=>item.left>=rect.left && item.right<=rect.right), above: mode.bottom<=label.top, width: card.scrollWidth, client: card.clientWidth};
        });
        assert.ok(layout.inside && layout.above && layout.width<=layout.client+1, JSON.stringify(layout));
        await page.screenshot({path:resolve(output, `selector-${locale}-${colorScheme}-${viewport.width}.png`)});
      }
      await page.locator('#pomodoroPanel summary').click();
      await page.locator('#timerModeSelect').selectOption('normal');
    }
    const beforeReload = (await page.locator('#timerDisplay').innerText()).trim();
    await page.reload({ waitUntil: 'networkidle' });
    // Native fixture persistence is independent of renderer checkpoints, like SQLite.
    await page.locator('#timerDisplay').filter({ hasText: beforeReload }).waitFor();
    assert.equal(await page.locator('#timerModeSelect').inputValue(), 'normal');
    await page.evaluate(() => {
      window.recordedHistory = { ...window.recordedHistory, date: '2026-10-02', total_seconds: 0, entries: [], category_breakdown: [], focus: { ...window.recordedHistory.focus, deep_focus_seconds: 0 } };
    });
    await page.clock.setSystemTime(new Date('2026-10-02T00:01:00+02:00'));
    await page.evaluate(() => window.testEmit('activity-report'));
    await page.locator('#timerDisplay').filter({ hasText: '00:00:00' }).waitFor();
    assert.deepEqual(errors, []);
    evidence.push({ locale, inflatedCheckpointIgnored: true, secondBySecond: true, analysisNotDoubleCounted: true, pomodoroCountdown: true, pomodoroBreakPausesTracking: true, pomodoroWaitsForUser: true, sharedTotals: true, savedObservationRefresh: true, pausePreserved: true, localMidnightReset: true, unchangedDomPreserved: true, insertedResourceReadingOffset: { before: topBefore, after: topAfter }, refreshFailureKeepsReport: true });
    console.log(`${locale}: 05:59:31 checkpoint → 05:45:00 recorded, live seconds, shared summary, no analysis double count, pause/resume, Pomodoro and midnight passed.`);
    await page.close();
  }
  await writeFile(resolve(output, 'verification.json'), JSON.stringify(evidence, null, 2));
} finally { await browser.close(); }
