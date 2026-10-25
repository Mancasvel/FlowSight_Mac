import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { mountEsp32Companion } from './esp32-companion.mjs';

const tick = () => new Promise(resolve => setImmediate(resolve));
async function mount(execute = async () => true) {
  const callbacks = new Map(), calls = [], events = new EventTarget();
  let language = 'es';
  const dispose = await mountEsp32Companion({
    events, execute,
    snapshot: () => ({ language, clock: '01:47:22' }),
    invoke: async (name, args) => { calls.push([name, args]); },
    listen: async (name, callback) => { callbacks.set(name, callback); return () => callbacks.delete(name); },
  });
  return { calls, events, callbacks, dispose, setLanguage: value => { language = value; } };
}

test('initial, native refresh and language events publish the app snapshot; disposal stops listeners', async () => {
  const m = await mount();
  assert.equal(m.calls[0][1].snapshot.clock, '01:47:22');
  await m.callbacks.get('esp32-companion-refresh')();
  m.setLanguage('en'); m.events.dispatchEvent(new Event('flowsight:languagechange')); await tick();
  assert.equal(m.calls.at(-1)[1].snapshot.language, 'en');
  m.dispose(); assert.equal(m.callbacks.size, 0);
  const count = m.calls.length;
  m.events.dispatchEvent(new Event('flowsight:languagechange')); await tick();
  assert.equal(m.calls.length, count);
});

test('overlapping controls cannot execute twice; acknowledgements preserve connection and request IDs', async () => {
  let release, executions = 0;
  const m = await mount(async () => { executions++; await new Promise(resolve => { release = resolve; }); return true; });
  const control = m.callbacks.get('esp32-companion-control');
  const first = control({ payload: { connection: 7, id: 10, action: 'pause' } });
  await control({ payload: { connection: 7, id: 11, action: 'pause' } });
  release(); await first;
  assert.equal(executions, 1);
  assert.deepEqual(m.calls.filter(([name]) => name === 'complete_esp32_companion_action').map(([, args]) => args), [
    { connection: 7, id: 11, ok: false }, { connection: 7, id: 10, ok: true },
  ]);
  m.dispose();
});

test('a failed or rejected app operation returns failure, leaving the app as authority', async () => {
  const m = await mount(async () => false);
  await m.callbacks.get('esp32-companion-control')({ payload: { connection: 2, id: 1, action: 'start' } });
  assert.equal(m.calls.at(-1)[1].ok, false);
  m.dispose();
});

const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const snapshotCode = html.slice(html.indexOf('    function esp32Snapshot('), html.indexOf('    async function esp32Action('));
test('snapshot copies rendered time, translated labels, goal, calendar, and Pomodoro break action', () => {
  const nodes = {
    timerModeSelect: { selectedOptions: [{ textContent: 'Normal' }], disabled: false },
    timerKindLabel: { textContent: 'Registrado hoy' }, timerDisplay: { textContent: '01:47:22' },
    timerGoalLabel: { textContent: '6 horas' }, playTimerLabel: { textContent: 'Iniciar registro' },
    playTimerBtn: { disabled: false },
  };
  const scope = {
    trackingStateHydrated: true, advancePomodoro: async () => {}, updateTimerDisplay() {}, updatePlayButtonState() {},
    totalFocusController: { session: () => null, busy: () => false }, timerPreferences: { mode: 'normal' },
    pomodoro: { phase: 'work', ready: true }, trackingTransitionInProgress: false, pomodoroTransition: false,
    setupInProgress: false, activeCalendarTask: () => ({ title: 'Project review' }), canUseCalendarCompanion: () => true,
    calendarStatus: { googleConnected: true }, isMonitoring: false, isPaused: false, getLanguage: () => 'es',
    document: { getElementById: id => nodes[id] }, tr: text => text === 'Ready' ? 'Listo' : text,
    getDisplayTotalSeconds: () => 6442, getDailyGoalHours: () => 6,
  };
  const snapshot = new Function('scope', `with(scope) { ${snapshotCode}; return esp32Snapshot; }`)(scope);
  const state = snapshot();
  assert.equal(state.clock, '01:47:22'); assert.equal(state.goal_value, '6 horas');
  assert.equal(state.goal_seconds, 21600); assert.equal(state.total_seconds, 6442);
  assert.equal(state.status, 'Listo'); assert.equal(state.language, 'es'); assert.equal(state.context, 'Project review');
  scope.timerPreferences.mode = 'pomodoro'; scope.pomodoro = { phase: 'short-break', ready: false };
  nodes.timerDisplay.textContent = '04:59'; nodes.playTimerBtn.disabled = true;
  assert.equal(snapshot().clock, '04:59'); assert.equal(snapshot().primary_action, 'end_break');
  assert.equal(snapshot().primary_enabled, true);
  scope.trackingStateHydrated = false; assert.equal(snapshot(), null);
});
const actionCode = html.slice(html.indexOf('    async function esp32Action('), html.indexOf('    mountEsp32Companion('));
function actionHarness(before) {
  const scope = {
    esp32Snapshot: () => before, isMonitoring: false, isPaused: true, pomodoro: { ready: false },
    startMonitoringWithNotice: async () => { scope.isMonitoring = true; },
    resumeMonitoring: async () => { scope.isMonitoring = true; scope.isPaused = false; },
    pauseMonitoring: async () => { scope.isMonitoring = false; scope.isPaused = true; },
    stopMonitoring: async () => { scope.isMonitoring = false; scope.isPaused = false; },
  };
  return { scope, action: new Function('scope', `with(scope) { ${actionCode}; return esp32Action; }`)(scope) };
}
test('stale pause cannot become resume, busy state blocks controls, and stop respects availability', async () => {
  const before = { busy: false, primary_enabled: true, primary_action: 'resume', stop_enabled: false };
  const h = actionHarness(before);
  assert.equal(await h.action('pause'), false);
  assert.equal(await h.action('stop'), false);
  before.busy = true; assert.equal(await h.action('resume'), false);
  before.busy = false; assert.equal(await h.action('resume'), true);
  assert.equal(h.scope.isMonitoring, true);
  before.stop_enabled = true; assert.equal(await h.action('stop'), true);
  assert.equal(h.scope.isPaused, false);
});
