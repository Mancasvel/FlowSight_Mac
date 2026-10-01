import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const authService = await readFile(new URL('./auth-service.js', import.meta.url), 'utf8');
const nativeSync = await readFile(new URL('../../src-tauri/src/sync.rs', import.meta.url), 'utf8');

test('optional cloud purposes remain visibly separate and off by default', () => {
  assert.match(html, /Optional sharing is off by default/);
  assert.match(html, /id="cloudSyncToggle"/);
  assert.match(html, /id="cloudAiToggle"/);
  assert.match(html, /id="analyticsConsentToggle"/);
  assert.match(html, /const PRIVACY_NOTICE_VERSION = '2026-08-23'/);
});

test('bearer credentials use masked transient inputs', () => {
  assert.match(html, /type="password" id="manualCodeInput"/);
  assert.match(html, /type="password" id="teamCodeInput"/);
  assert.match(html, /codeInput\.value = ''/);
  assert.match(html, /getElementById\('teamCodeInput'\)\.value = ''/);
});

test('renderer auth does not persist a second plaintext session', () => {
  assert.match(authService, /persistSession:\s*false/);
  assert.doesNotMatch(authService, /localStorage\.setItem/);
  assert.match(html, /localStorage\.removeItem\('flowsight_team_code'\)/);
  assert.doesNotMatch(html, /localStorage\.setItem\('flowsight_team_code'/);
});

test('diagnostics do not print integration task payloads or account identifiers', () => {
  assert.doesNotMatch(html, /console\.log\('\[UI\] Jira tasks:',\s*tasks/);
  assert.doesNotMatch(html, /console\.log\('\[UI\] Linear tasks:',\s*tasks/);
  assert.doesNotMatch(html, /Restored linked session for:',\s*profile\.display_name/);
  assert.doesNotMatch(html, /Switched active team to:',\s*teamId/);
  assert.doesNotMatch(nativeSync, /auto-selecting first membership: \{\}/);
});

test('remote profile images suppress referrer disclosure', () => {
  assert.match(html, /id="userAvatar"[^>]*referrerpolicy="no-referrer"/);
});
