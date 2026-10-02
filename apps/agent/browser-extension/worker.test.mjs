import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { webcrypto } from 'node:crypto';

const source = readFileSync(new URL('./worker.js', import.meta.url), 'utf8');

function harness(fetchResponse = async () => { throw new Error('offline'); }) {
  const stored = {};
  const changes = [];
  const listeners = { addListener() {} };
  const chrome = {
    action: { onClicked: listeners },
    alarms: { create() {}, clear() {}, onAlarm: listeners },
    runtime: { onInstalled: listeners, onStartup: listeners, onMessage: listeners, getManifest() { return { version: '1.1.1' }; }, getURL(path) { return `chrome-extension://test/${path}`; }, openOptionsPage() {} },
    storage: { local: {
      async get(names) {
        const keys = Array.isArray(names) ? names : [names];
        return Object.fromEntries(keys.filter((key) => key in stored).map((key) => [key, stored[key]]));
      },
      async set(value) { Object.assign(stored, value); },
    } },
    tabs: { onUpdated:listeners, async query() { return []; }, async update() {} },
    declarativeNetRequest: {
      async getDynamicRules() { return changes.reduce((rules, change) => [...rules.filter(rule => !change.removeRuleIds?.includes(rule.id)), ...(change.addRules || [])], []); },
      async updateDynamicRules(change) { changes.push(change); }
    },
  };
  const context = { chrome, URL, crypto: webcrypto, fetch: fetchResponse,
    AbortSignal, console, setTimeout, clearTimeout };
  runInNewContext(`${source}\nglobalThis.__test = { ruleFor, runCommand, expireBlocks, reconcileFocus, focusStatus, cancelFocus, releaseBlocksAfterDisconnect, matchesFocus, poll };`, context);
  return { ...context.__test, stored, changes };
}

test('a URL path block matches the chosen site and route only', () => {
  const { ruleFor } = harness();
  const rule = ruleFor('youtube.com/shorts', 10001);
  const pattern = new RegExp(rule.condition.regexFilter);
  assert.equal(pattern.test('https://www.youtube.com/shorts/abc'), true);
  assert.equal(pattern.test('https://youtube.com/watch?v=1'), false);
  assert.equal(pattern.test('https://notyoutube.com/shorts/abc'), false);
  assert.throws(() => ruleFor('file:///etc/passwd', 10002));
  assert.throws(() => ruleFor('https://evil.example/?secret=1', 10003));
});

test('temporary blocks can be removed and do not survive their expiry', async () => {
  const { runCommand, expireBlocks, stored, changes } = harness();
  const result = await runCommand('browser.block', { patterns: ['youtube.com/shorts'], duration_minutes: 1 });
  assert.deepEqual(Array.from(result.blocked), ['youtube.com/shorts']);
  assert.equal(stored.blocks.length, 1);
  assert.equal(changes[0].addRules[0].action.type, 'block');
  await runCommand('browser.unblock', { patterns: ['youtube.com/shorts'] });
  assert.equal(stored.blocks.length, 0);
  await runCommand('browser.block', { patterns: ['youtube.com'], duration_minutes: 1 });
  stored.blocks[0].expiresAt = Date.now() - 1;
  await expireBlocks();
  assert.equal(stored.blocks.length, 0);
  assert.equal(changes.at(-1).removeRuleIds.length, 1);
});

const policy = (overrides = {}) => ({id:'focus-1',intention:'ADDA exercise',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['youtube.com','instagram.com'],exceptions:['youtube.com/watch'],...overrides});

test('total focus uses owned rules, exceptions take priority, and legacy unblocking preserves it', async () => {
  const h=harness(); await h.reconcileFocus(policy());
  const rules=h.changes.at(-1).addRules;
  assert.equal(rules[0].action.type,'redirect');assert.equal(rules[2].action.type,'allow');
  assert.ok(rules[2].priority>rules[0].priority);
  assert.equal(new RegExp(rules[0].condition.regexFilter).test('https://m.youtube.com/shorts/1'),true);
  assert.equal(new RegExp(rules[0].condition.regexFilter).test('https://notyoutube.com'),false);
  await h.runCommand('browser.block',{patterns:['reddit.com'],duration_minutes:1});
  await h.runCommand('browser.unblock_all',{});
  assert.equal((await h.focusStatus()).applied,true);
  await h.reconcileFocus(null);assert.equal((await h.focusStatus()).applied,false);
});

test('focus persists on disconnect, reconciles restart, and expires independently',async()=>{
  const h=harness();const p=policy();await h.reconcileFocus(p);
  h.stored.lastConnectedAt=0;await h.releaseBlocksAfterDisconnect();
  assert.equal((await h.focusStatus()).sessionId,p.id);
  const before=h.changes.length;await h.reconcileFocus(p);assert.equal(h.changes.length,before);
  // Simulate rule loss while extension storage survives a restart.
  h.changes.push({removeRuleIds:[1000,1001,1020]});await h.reconcileFocus(p);
  assert.equal((await h.focusStatus()).applied,true);
  h.stored.focus.expiresAt=new Date(Date.now()-1).toISOString();await h.expireBlocks();
  assert.equal((await h.focusStatus()).applied,false);
});

test('emergency end cannot be undone by a stale native policy',async()=>{
  const h=harness();const p=policy();await h.reconcileFocus(p);await h.cancelFocus();
  await h.reconcileFocus(p);assert.equal((await h.focusStatus()).applied,false);
  assert.equal((await h.focusStatus()).cancelledSessionId,p.id);
  await h.reconcileFocus(null);await h.reconcileFocus(policy({id:'focus-2'}));
  assert.equal((await h.focusStatus()).applied,true);
});

test('invalid focus policy changes no rules and does not send messages',async()=>{
  const h=harness();
  for(const patterns of [[],['127.0.0.1'],['file:///etc/passwd'],['https://example.com?secret=1']]) {
    await assert.rejects(()=>h.reconcileFocus(policy({patterns})));
  }
  assert.equal(h.changes.length,0);
  await assert.rejects(()=>h.runCommand('messages.auto_reply',{}),/Unsupported/);
});

test('total focus normalizes common www input and catches internal route changes',()=>{
  const h=harness();const p=policy({patterns:['www.youtube.com/shorts'],exceptions:['youtube.com/shorts/lesson']});
  assert.equal(h.matchesFocus('https://m.youtube.com/shorts/1',p),true);
  assert.equal(h.matchesFocus('https://youtube.com/shorts/lesson/1',p),false);
  assert.equal(h.matchesFocus('https://www.youtube.com/watch?v=1',p),false);
  assert.equal(h.matchesFocus('https://notyoutube.com/shorts/1',p),false);
});

test('pairing reports a rejected key and a confirmed MV3 focus handshake', async () => {
  let code = 401;
  const h = harness(async () => ({ ok: code === 200, status: code, json: async () => ({command: null, focus: null}) }));
  await h.poll();
  Object.assign(h.stored, {port: 38547, token: 'fixture'});
  const rejected = await h.poll();
  assert.equal(rejected.connected, false);
  assert.match(rejected.error, /pairing key was rejected/);
  code = 200;
  assert.equal((await h.poll()).connected, true);
  const status = await h.focusStatus();
  assert.equal(status.extensionVersion, '1.1.1');
  assert.equal(status.applied, false);
});

test('simultaneous pairing checks share one authenticated poll', async () => {
  let requests = 0;
  const h = harness(async () => { requests++; return {ok: true, json: async () => ({command: null, focus: null})}; });
  await h.poll();
  Object.assign(h.stored, {port: 38547, token: 'fixture'});
  const results = await Promise.all([h.poll(), h.poll(), h.poll()]);
  assert.ok(results.every(result => result.connected));
  assert.equal(requests, 2, 'One next request and one focus status acknowledgement');
});
