import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, localizeStatus, setLanguagePreference } from './i18n.mjs';
setLanguagePreference('en',{persist:false});
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const renderer = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const start = renderer.indexOf('    async function stopNativeMonitoring(');
const end = renderer.indexOf('    async function resumeMonitoring()', start);
assert.ok(start >= 0 && end > start, 'tracking controls must be present in the renderer');
const controlsSource = renderer.slice(start, end);

function createHarness({ monitoring = true, paused = false, invoke } = {}) {
  const calls = [];
  const buttons = {
    playTimerBtn: { disabled: false },
    stopTimerBtn: { disabled: false },
  };
  const context = {
    tr, formatMessage, html, markup, setText, setAttributeText, localizeStatus,
    isMonitoring: monitoring,
    isPaused: paused,
    trackingTransitionInProgress: false,
    document: { getElementById: id => buttons[id] ?? null },
    invoke: invoke ?? (async command => { calls.push(command); }),
    commitSessionTime: () => calls.push('commitSessionTime'),
    renderTrackingStatus: () => calls.push('renderTrackingStatus'),
    persistTrackingState: () => calls.push('persistTrackingState'),
    refreshTodayView: async () => { calls.push('refreshTodayView'); },
    showToast: (message, kind) => calls.push(`toast:${kind}:${message}`),
    console: { error: () => {}, warn: () => {} },
  };
  runInNewContext(`${controlsSource}\nglobalThis.controls = { pauseMonitoring, stopMonitoring };`, context);
  return { context, buttons, calls, ...context.controls };
}

test('the visible controls still delegate to the pause and stop handlers', () => {
  assert.match(renderer, /getElementById\('stopTimerBtn'\)\?\.addEventListener\('click',\s*\(\) => \{\s*document\.getElementById\('stopBtn'\)\?\.click\(\)/);
  assert.match(renderer, /if \(isMonitoring\) \{\s*document\.getElementById\('pauseBtn'\)\?\.click\(\)/);
});

test('pause stops native monitoring and preserves the session time', async () => {
  const harness = createHarness();
  await harness.pauseMonitoring();

  assert.equal(harness.context.isMonitoring, false);
  assert.equal(harness.context.isPaused, true);
  assert.deepEqual(harness.calls.slice(0, 5), [
    'get_local_agent_data',
    'stop_monitoring',
    'commitSessionTime',
    'renderTrackingStatus',
    'persistTrackingState',
  ]);
  assert.equal(harness.calls.some(call => call === 'stop_server'), false);
  assert.equal(harness.buttons.playTimerBtn.disabled, false);
  assert.equal(harness.buttons.stopTimerBtn.disabled, false);
});

test('stop ends native monitoring, shuts down the server, and refreshes today', async () => {
  const harness = createHarness();
  await harness.stopMonitoring();

  assert.equal(harness.context.isMonitoring, false);
  assert.equal(harness.context.isPaused, false);
  assert.deepEqual(harness.calls.slice(0, 7), [
    'get_local_agent_data',
    'stop_monitoring',
    'commitSessionTime',
    'renderTrackingStatus',
    'persistTrackingState',
    'stop_server',
    'toast:success:Tracking stopped',
  ]);
  assert.equal(harness.calls.at(-1), 'refreshTodayView');
  assert.equal(harness.buttons.playTimerBtn.disabled, false);
  assert.equal(harness.buttons.stopTimerBtn.disabled, false);
});

test('pausing an agent focus block releases its protections through the focus action', async () => {
  const harness = createHarness({
    invoke: async (command, args) => {
      harness.calls.push(command === 'control_local_focus_block' ? `${command}:${args.action}` : command);
      if (command === 'get_local_agent_data') return { focus: { status: 'running' } };
    },
  });
  await harness.pauseMonitoring();
  assert.ok(harness.calls.includes('control_local_focus_block:pause'));
  assert.equal(harness.calls.includes('stop_monitoring'), false);
  assert.equal(harness.context.isPaused, true);
});

test('a native stop failure keeps the running state and reports the error', async () => {
  const harness = createHarness({
    invoke: async command => {
      harness.calls.push(command);
      throw new Error('native unavailable');
    },
  });
  await harness.pauseMonitoring();

  assert.equal(harness.context.isMonitoring, true);
  assert.equal(harness.context.isPaused, false);
  assert.equal(harness.calls.includes('commitSessionTime'), false);
  assert.match(harness.calls.at(-1), /Could not pause tracking: Error: native unavailable/);
  assert.equal(harness.buttons.playTimerBtn.disabled, false);
});

test('a server shutdown failure still reports that tracking stopped', async () => {
  const harness = createHarness({
    invoke: async command => {
      harness.calls.push(command);
      if (command === 'stop_server') throw new Error('server busy');
    },
  });
  await harness.stopMonitoring();

  assert.equal(harness.context.isMonitoring, false);
  assert.equal(harness.context.isPaused, false);
  assert.ok(harness.calls.some(call => /server could not shut down: Error: server busy/.test(call)));
  assert.equal(harness.calls.at(-1), 'refreshTodayView');
});

test('stop from a paused session can still shut down the server if native monitoring is unavailable', async () => {
  const harness = createHarness({
    monitoring: false,
    paused: true,
    invoke: async command => {
      harness.calls.push(command);
      if (command === 'stop_monitoring') throw new Error('native unavailable');
    },
  });
  await harness.stopMonitoring();

  assert.equal(harness.context.isPaused, false);
  assert.ok(harness.calls.includes('stop_server'));
  assert.ok(harness.calls.includes('toast:success:Tracking stopped'));
});

test('repeated clicks cannot start overlapping pause and stop transitions', async () => {
  let finishStop;
  const nativeStop = new Promise(resolve => { finishStop = resolve; });
  const harness = createHarness({
    invoke: async command => {
      harness.calls.push(command);
      if (command === 'stop_monitoring') await nativeStop;
    },
  });
  const first = harness.pauseMonitoring();
  await harness.pauseMonitoring();
  await harness.stopMonitoring();
  assert.equal(harness.calls.filter(call => call === 'stop_monitoring').length, 1);
  assert.equal(harness.buttons.playTimerBtn.disabled, true);
  finishStop();
  await first;
  assert.equal(harness.context.isPaused, true);
  assert.equal(harness.buttons.playTimerBtn.disabled, false);
});
