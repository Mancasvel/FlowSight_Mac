import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, localizeStatus, setLanguagePreference } from './i18n.mjs';
setLanguagePreference('en',{persist:false});
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const renderer = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const start = renderer.indexOf('    const UPDATE_CHECK_INTERVAL_MS =');
const end = renderer.indexOf('    async function loadConfig()', start);
assert.ok(start >= 0 && end > start, 'updater functions must be present in the renderer');
const updaterSource = renderer.slice(start, end);

function createHarness() {
  let now = 1_000_000_000;
  let updateResult = null;
  let checks = 0;
  let relaunches = 0;
  const storage = new Map();
  const intervals = [];
  const windowListeners = new Map();
  const documentListeners = new Map();
  const toasts = [];
  let overlay = null;

  const document = {
    activeElement: null,
    hidden: false,
    getElementById(id) {
      if (id === 'checkUpdatesBtn') return checkButton;
      if (id === 'updateOverlay') return overlay;
      return null;
    },
    createElement() {
      const element = makeNode('updateOverlay');
      const children = new Map();
      element.querySelector = selector => {
        if (!children.has(selector)) children.set(selector, makeNode(selector));
        return children.get(selector);
      };
      element.remove = () => { overlay = null; };
      return element;
    },
    body: {
      appendChild(element) { overlay = element; },
    },
    addEventListener(type, listener) { documentListeners.set(type, listener); },
  };

  function makeNode(id) {
    const listeners = new Map();
    const attributes = new Map();
    return {
      id,
      hidden: false,
      disabled: false,
      textContent: '',
      innerHTML: '',
      style: {},
      attributes,
      setAttribute(name, value) { attributes.set(name, value); },
      addEventListener(type, listener) { listeners.set(type, listener); },
      focus() { document.activeElement = this; },
      fire(type, event = {}) { return listeners.get(type)?.(event); },
    };
  }

  const checkButton = makeNode('checkUpdatesBtn');
  checkButton.textContent = 'Check for updates';
  checkButton.focus();
  const context = {
    tr, formatMessage, html, markup, setText, setAttributeText, localizeStatus,
    installationRepair: { needsRepair: () => false },
    Date: { now: () => now },
    document,
    window: { addEventListener: (type, listener) => windowListeners.set(type, listener) },
    localStorage: {
      getItem: key => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, value),
    },
    setInterval: (callback, ms) => { intervals.push({ callback, ms }); },
    checkAppUpdate: async () => {
      checks += 1;
      return typeof updateResult === 'function' ? updateResult() : updateResult;
    },
    getVersion: async () => '4.1.2',
    showToast: (...args) => toasts.push(args),
    escapeHtml: value => value,
    relaunch: async () => { relaunches += 1; },
    console: { warn() {}, error() {} },
  };
  runInNewContext(`${updaterSource}\nglobalThis.updater = { checkForUpdates, startAutomaticUpdateChecks };`, context);

  return {
    updater: context.updater,
    checkButton,
    document,
    intervals,
    toasts,
    storage,
    setUpdateResult(value) { updateResult = value; },
    advance(ms) { now += ms; },
    emitFocus() { windowListeners.get('focus')?.(); },
    emitVisibility() { documentListeners.get('visibilitychange')?.(); },
    getOverlay() { return overlay; },
    get checks() { return checks; },
    get relaunches() { return relaunches; },
  };
}

function makeUpdate(overrides = {}) {
  return {
    version: '4.1.3',
    body: 'A focused update.',
    close: async () => {},
    downloadAndInstall: async () => {},
    ...overrides,
  };
}

const flush = () => new Promise(resolve => setImmediate(resolve));

test('automatic checks start after installation verification and repeat while open', async () => {
  const installCheck = renderer.indexOf('installationCheckPromise = installationRepair.check();');
  const nativeRepairSignal = renderer.indexOf("listen('installation-repair-required'", installCheck);
  const updateCheck = renderer.indexOf('installationCheckPromise.finally(() => startAutomaticUpdateChecks());', nativeRepairSignal);
  const appInit = renderer.indexOf('    init();', updateCheck);
  assert.ok(installCheck >= 0 && nativeRepairSignal > installCheck && updateCheck > nativeRepairSignal && appInit > updateCheck);
  const harness = createHarness();
  harness.updater.startAutomaticUpdateChecks();
  await flush();

  assert.equal(harness.checks, 1);
  assert.equal(harness.intervals.length, 1);
  assert.equal(harness.intervals[0].ms, 15 * 60 * 1000);

  harness.advance(5 * 60 * 1000 - 1);
  harness.emitFocus();
  await flush();
  assert.equal(harness.checks, 1, 'focus checks are throttled');

  harness.advance(1);
  harness.emitFocus();
  await flush();
  assert.equal(harness.checks, 2);

  harness.advance(15 * 60 * 1000);
  harness.intervals[0].callback();
  await flush();
  assert.equal(harness.checks, 3);
});

test('Later postpones automatic prompts for a day but manual checks still show them', async () => {
  const harness = createHarness();
  let closed = 0;
  harness.setUpdateResult(() => makeUpdate({ close: async () => { closed += 1; } }));
  await harness.updater.checkForUpdates();

  const overlay = harness.getOverlay();
  assert.ok(overlay);
  assert.match(overlay.innerHTML, /class="button button-secondary" id="updateLaterBtn"/);
  assert.match(overlay.innerHTML, /class="button button-primary" id="updateNowBtn"/);
  assert.doesNotMatch(overlay.innerHTML, /style=/);
  assert.equal(overlay.attributes.get('aria-modal'), 'true');
  overlay.querySelector('#updateLaterBtn').fire('click');

  assert.equal(harness.getOverlay(), null);
  assert.equal(closed, 1);
  assert.ok(harness.storage.has('flowsight_update_snooze'));

  await harness.updater.checkForUpdates();
  assert.equal(harness.getOverlay(), null);
  assert.equal(closed, 2, 'snoozed updater resources are released');

  await harness.updater.checkForUpdates({ manual: true });
  assert.ok(harness.getOverlay(), 'manual checks override Later');
});

test('Update now shows progress and calls relaunch after installation', async () => {
  const harness = createHarness();
  harness.setUpdateResult(makeUpdate({
    downloadAndInstall: async onEvent => {
      onEvent({ event: 'Started', data: { contentLength: 100 } });
      onEvent({ event: 'Progress', data: { chunkLength: 40 } });
      onEvent({ event: 'Progress', data: { chunkLength: 60 } });
      onEvent({ event: 'Finished' });
    },
  }));
  await harness.updater.checkForUpdates();

  const overlay = harness.getOverlay();
  await overlay.querySelector('#updateNowBtn').fire('click');
  assert.equal(overlay.querySelector('#updateActions').hidden, true);
  assert.equal(overlay.querySelector('#updateProgress').hidden, false);
  assert.equal(overlay.querySelector('#updateProgressBar').style.transform, 'scaleX(1)');
  assert.equal(overlay.querySelector('#updateProgressTrack').attributes.get('aria-valuenow'), '100');
  assert.equal(harness.relaunches, 1);
});

test('the dialog exposes long details to keyboard users and traps focus', async () => {
  const harness = createHarness();
  harness.setUpdateResult(makeUpdate());
  await harness.updater.checkForUpdates();

  const overlay = harness.getOverlay();
  const details = overlay.querySelector('.update-dialog__body');
  const nowButton = overlay.querySelector('#updateNowBtn');
  assert.match(overlay.innerHTML, /class="modal-body update-dialog__body" tabindex="0" role="region"/);

  details.focus();
  const backward = { key: 'Tab', shiftKey: true, preventDefault() { this.prevented = true; } };
  overlay.fire('keydown', backward);
  assert.equal(backward.prevented, true);
  assert.equal(harness.document.activeElement, nowButton);

  const forward = { key: 'Tab', shiftKey: false, preventDefault() { this.prevented = true; } };
  overlay.fire('keydown', forward);
  assert.equal(forward.prevented, true);
  assert.equal(harness.document.activeElement, details);
});

test('installation errors remain visible inside the dialog and allow retry', async () => {
  const harness = createHarness();
  harness.setUpdateResult(makeUpdate({
    downloadAndInstall: async () => { throw new Error('network unavailable'); },
  }));
  await harness.updater.checkForUpdates();

  const overlay = harness.getOverlay();
  const nowButton = overlay.querySelector('#updateNowBtn');
  await nowButton.fire('click');
  assert.equal(overlay.querySelector('#updateActions').hidden, false);
  assert.equal(overlay.querySelector('#updateProgress').hidden, true);
  assert.equal(overlay.querySelector('#updateError').hidden, false);
  assert.match(overlay.querySelector('#updateError').textContent, /Try again or use the official GitHub release/);
  assert.equal(harness.document.activeElement, nowButton);
});
