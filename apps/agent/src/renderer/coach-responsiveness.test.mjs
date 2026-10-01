import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const commands = readFileSync(new URL('../../src-tauri/src/coach_chat.rs', import.meta.url), 'utf8');
const markup = readFileSync(new URL('./index.html', import.meta.url), 'utf8');

test('Coach network requests run off the Tauri UI thread', () => {
  assert.match(commands, /pub async fn get_coach_chat_usage\(\)[\s\S]*?spawn_blocking\(get_coach_chat_usage_blocking\)/);
  assert.match(commands, /pub async fn send_coach_chat_message\(message: String\)[\s\S]*?spawn_blocking\(move \|\| send_coach_chat_message_blocking\(message\)\)/);
  assert.match(commands, /fn send_coach_chat_message_blocking\(message: String\)[\s\S]*?build_local_insights_report[\s\S]*?\.send\(\)/);
});

test('waiting for Coach disables only its send control, not navigation', () => {
  const send = markup.slice(markup.indexOf('async function sendCoachMessage'), markup.indexOf("document.getElementById('coachComposerForm')?.addEventListener"));
  assert.match(send, /button\.disabled = true/);
  assert.match(send, /button\.disabled = false/);
  assert.doesNotMatch(send, /nav\.disabled|nav-item.*disabled|pointerEvents|cursor\s*=\s*['"]none/);
});
