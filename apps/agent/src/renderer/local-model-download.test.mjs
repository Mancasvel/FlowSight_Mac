import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const html = readFileSync(new URL('./index.html', import.meta.url), 'utf8');

test('first-use model download is awaited before the local server starts', () => {
  const setup = html.slice(html.indexOf('async function startMonitoringFull()'));
  const download = setup.indexOf('await ensureLocalModelDownloaded(showStep)');
  const start = setup.indexOf("await invoke('start_server')");
  assert.ok(download >= 0 && start > download);
  assert.match(html, /invoke\('local_model_status'\)/);
  assert.match(html, /listen\('local-model-download'/);
});

test('a failed first-use download keeps a retry action visible', () => {
  assert.match(html, /id="setupRetryBtn"/);
  assert.match(html, /id="setupDismissBtn"/);
  const setup = html.slice(html.indexOf('async function startMonitoringFull()'));
  assert.match(setup, /setupFailed = true/);
  assert.match(setup, /if \(!setupFailed\) overlay\.hidden = true/);
});
