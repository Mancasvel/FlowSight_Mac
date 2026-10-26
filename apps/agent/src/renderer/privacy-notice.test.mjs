import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { monitoringNoticeAccepted } from './privacy-notice.mjs';

const renderer = readFileSync(new URL('./index.html', import.meta.url), 'utf8');

function harness(settings, read) {
  let starts = 0;
  let notices = 0;
  let errors = 0;
  const context = {
    privacySettings: settings,
    monitoringNoticeAccepted,
    invoke: read,
    pendingMonitoringStart: false,
    renderPrivacySettings() {},
    startMonitoringFull: async () => { starts += 1; },
    showMonitoringNotice: () => { notices += 1; },
    showToast: () => { errors += 1; },
    tr: value => value,
    console: { warn() {} },
  };
  const load = renderer.slice(renderer.indexOf('    async function loadPrivacySettings()'), renderer.indexOf('    async function applyPrivacyPatch('));
  const start = renderer.slice(renderer.indexOf('    async function startMonitoringWithNotice()'), renderer.indexOf("    document.getElementById('monitoringNoticeCancelBtn')"));
  runInNewContext(`${load}\n${start}\nglobalThis.start = startMonitoringWithNotice;`, context);
  return { context, get starts() { return starts; }, get notices() { return notices; }, get errors() { return errors; } };
}

test('a saved acceptance from either existing build starts without asking again after every reopen', async () => {
  for (const noticeVersion of ['2026-08-23', '2026-09-28']) {
    const saved = { noticeVersion, monitoringNoticeAcknowledged: true };
    for (let reopen = 0; reopen < 3; reopen += 1) {
      const ui = harness(null, async () => structuredClone(saved));
      await ui.context.start();
      assert.equal(ui.starts, 1);
      assert.equal(ui.notices, 0);
    }
  }
});

test('fresh, withdrawn, and unknown notices still ask for acceptance', async () => {
  for (const saved of [
    { noticeVersion: '2026-08-23', monitoringNoticeAcknowledged: false },
    { noticeVersion: '2026-09-28', monitoringNoticeAcknowledged: false },
    { noticeVersion: '2099-01-01', monitoringNoticeAcknowledged: true },
  ]) {
    const ui = harness(null, async () => saved);
    await ui.context.start();
    assert.equal(ui.notices, 1);
    assert.equal(ui.starts, 0);
  }
});

test('a failed first read remains retryable instead of showing a false consent prompt', async () => {
  let fail = true;
  const saved = { noticeVersion: '2026-09-28', monitoringNoticeAcknowledged: true };
  const ui = harness(null, async () => {
    if (fail) throw new Error('database temporarily unavailable');
    return saved;
  });
  await ui.context.start();
  assert.equal(ui.notices, 0);
  assert.equal(ui.starts, 0);
  assert.equal(ui.errors, 1);
  fail = false;
  await ui.context.start();
  assert.equal(ui.starts, 1);
  assert.equal(ui.notices, 0);
});

test('an existing acceptance remains in memory during a temporary read failure', async () => {
  const saved = { noticeVersion: '2026-09-28', monitoringNoticeAcknowledged: true };
  const ui = harness(saved, async () => { throw new Error('busy'); });
  await ui.context.start();
  assert.equal(ui.context.privacySettings, saved);
  assert.equal(ui.notices, 0);
  assert.equal(ui.starts, 1);
});

test('tracking retries the stored choice and honors withdrawal made by another window', async () => {
  const ui = harness(
    { noticeVersion: '2026-08-23', monitoringNoticeAcknowledged: true },
    async () => ({ noticeVersion: '2026-08-23', monitoringNoticeAcknowledged: false }),
  );
  await ui.context.start();
  assert.equal(ui.starts, 0);
  assert.equal(ui.notices, 1);
});
