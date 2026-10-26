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
    permissions: { async contains() { return true; } },
    action: { onClicked: listeners },
    alarms: { create() {}, clear() {}, onAlarm: listeners },
    runtime: { onInstalled: listeners, onStartup: listeners, onMessage: listeners, getManifest() { return { version: '1.1.4' }; }, getURL(path) { return `chrome-extension://test/${path}`; }, openOptionsPage() {} },
    storage: { local: {
      async get(names) {
        const keys = Array.isArray(names) ? names : [names];
        return Object.fromEntries(keys.filter((key) => key in stored).map((key) => [key, stored[key]]));
      },
      async set(value) { Object.assign(stored, value); },
    } },
    tabs: { onUpdated:listeners, async query() { return []; }, async update() {},
      async get(id) { return { id, url: stored.focusMusic?.url, status: 'complete' }; }, async sendMessage() { return {}; } },
    declarativeNetRequest: {
      async getDynamicRules() { return changes.reduce((rules, change) => [...rules.filter(rule => !change.removeRuleIds?.includes(rule.id)), ...(change.addRules || [])], []); },
      async updateDynamicRules(change) { changes.push(change); }
    },
  };
  const context = { chrome, URL, crypto: webcrypto, fetch: fetchResponse,
    AbortSignal, console, setTimeout, clearTimeout };
  runInNewContext(`${source}\nglobalThis.__test = { ruleFor, runCommand, expireBlocks, reconcileFocus, focusStatus, cancelFocus, releaseBlocksAfterDisconnect, matchesFocus, poll };`, context);
  return { ...context.__test, stored, changes, chrome };
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

test('YouTube playback follows pause and resume in the owned tab across policy IDs', async () => {
  const h = harness(), messages = [], created = [];
  h.chrome.windows = { async getAll() { return [{ id: 24, focused: true }]; } };
  h.chrome.tabs.create = async options => { created.push(options); return { id: 50 }; };
  h.chrome.tabs.sendMessage = async (id, message) => { messages.push({ id, ...message }); return {}; };
  const musicUrl = 'https://www.youtube.com/watch?v=music&list=123';
  await h.reconcileFocus(policy({ musicUrl }));
  await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'focus-1' });
  assert.equal(messages.at(-1).playback, 'play');
  await h.reconcileFocus(null);
  await h.runCommand('browser.focus_status', { musicPlayback: 'pause', musicSessionId: 'stale', sessionId: 'stale' });
  assert.equal(messages.length, 1, 'Stale sessions cannot control music');
  await h.runCommand('browser.focus_status', { musicPlayback: 'pause', musicSessionId: 'focus-1', sessionId: 'focus-1' });
  assert.equal(messages.at(-1).playback, 'pause');
  await h.reconcileFocus(policy({ id: 'resumed', musicUrl }));
  await h.runCommand('browser.focus_status', { musicPlayback: 'play', musicSessionId: 'focus-1', sessionId: 'resumed' });
  assert.equal(messages.at(-1).playback, 'play');
  assert.equal(h.stored.focusMusic.sessionId, 'resumed');
  assert.equal(created.length, 1);
  assert.ok(messages.every(message => message.id === 50));
  const count = messages.length;
  await h.runCommand('browser.focus_status', {});
  assert.equal(messages.length, count, 'Status polls do not override user playback');
  await h.reconcileFocus(null);
  await h.runCommand('browser.focus_status', { musicPlayback: 'release', musicSessionId: 'resumed', sessionId: 'resumed' });
  assert.equal(messages.at(-1).playback, 'release');
  assert.equal(h.stored.focusMusic, null);
});

test('loading, closed and navigated music tabs do not break protection or create duplicates', async () => {
  const h = harness();
  h.chrome.windows = { async getAll() { return [{ id: 24 }]; } };
  h.chrome.tabs.create = async () => ({ id: 50 });
  const musicUrl = 'https://music.youtube.com/watch?v=music';
  await h.reconcileFocus(policy({ musicUrl }));
  h.chrome.tabs.get = async () => ({ id: 50, url: musicUrl, status: 'loading' });
  h.chrome.tabs.sendMessage = async () => { throw new Error('Receiving end does not exist'); };
  const opening = await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'focus-1' });
  assert.equal(opening.musicError, undefined);
  await h.reconcileFocus(null);
  const pending = await h.runCommand('browser.focus_status', { musicPlayback: 'pause', musicSessionId: 'focus-1', sessionId: 'focus-1' });
  assert.equal(pending.musicPending, true);
  assert.equal(h.stored.focusMusic.playback, 'pause');
  h.chrome.tabs.get = async () => ({ id: 50, url: 'https://unrelated.example.com', status: 'complete' });
  const navigated = await h.runCommand('browser.focus_status', { musicPlayback: 'pause', musicSessionId: 'focus-1', sessionId: 'focus-1' });
  assert.match(navigated.musicError, /no longer on YouTube/);
  h.chrome.tabs.get = async () => { throw new Error('No tab with id: 50'); };
  await h.reconcileFocus(policy({ id: 'resumed', musicUrl }));
  const closed = await h.runCommand('browser.focus_status', { musicPlayback: 'play', musicSessionId: 'focus-1', sessionId: 'resumed' });
  assert.equal(closed.applied, true);
  assert.match(closed.musicError, /No tab/);
});

test('music opens once in an inactive tab without changing window or tab focus', async () => {
  const h = harness(), created = [];
  h.chrome.windows = { async getAll() { return [{ id: 12, focused: false }, { id: 24, focused: true }]; } };
  h.chrome.tabs.create = async options => { created.push(options);return { id: 50 }; };
  const musicUrl = 'https://youtube.com/watch?v=music&list=123#t=20';
  await h.reconcileFocus(policy({ musicUrl }));
  assert.equal(h.matchesFocus(musicUrl, h.stored.focus), false);
  assert.equal(h.matchesFocus('https://youtube.com/shorts/123', h.stored.focus), true);
  assert.equal(h.matchesFocus('http://localhost:1420', h.stored.focus), false);
  const allow = h.changes.at(-1).addRules.find(rule => rule.id === 1040);
  assert.equal(new RegExp(allow.condition.regexFilter).test(musicUrl.split('#')[0]), true);
  assert.equal(new RegExp(allow.condition.regexFilter).test('https://youtube.com/watch?v=other'), false);
  assert.equal(allow.condition.isUrlFilterCaseSensitive, true);
  await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'stale' });
  assert.equal(created.length, 0);
  assert.equal((await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'focus-1' })).musicOpened, true);
  await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'focus-1' });
  assert.equal(created.length, 1);
  assert.equal(created[0].url, musicUrl);assert.equal(created[0].windowId, 24);assert.equal(created[0].active, false);
  await h.reconcileFocus(null);
  await h.reconcileFocus(policy({ id: 'resumed', musicUrl }));
  await h.runCommand('browser.focus_status', {});
  assert.equal(created.length, 1, 'Resume/status/reconnect never opens another music tab');
});

test('music never creates a browser window and reports failure while focus remains applied', async () => {
  const h = harness();
  h.chrome.windows = { async getAll() { return []; } };
  h.chrome.tabs.create = async () => { throw new Error('Must not create a tab without an existing window'); };
  await h.reconcileFocus(policy({ musicUrl: 'https://music.example.com/playlist?list=123' }));
  const result = await h.runCommand('browser.focus_status', { openMusic: true, sessionId: 'focus-1' });
  assert.equal(result.applied, true);assert.match(result.musicError, /Keep a browser window open/);
  assert.equal(h.stored.musicOpenedSessionId, undefined);
});

test('unsafe music URLs cannot reach browser tab creation', async () => {
  for (const musicUrl of ['javascript:alert(1)', 'file:///music.mp3', 'https://user:pass@music.example.com', 'http://127.0.0.1', 'https://music.example.com:8443', 'https://music.example.com/a b']) {
    const h = harness();
    await assert.rejects(h.reconcileFocus(policy({ musicUrl })));
    assert.equal(h.changes.length, 0);
  }
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
  assert.equal(status.extensionVersion, '1.1.4');
  assert.equal(status.applied, false);
});

test('missing site access keeps pairing connected without claiming focus readiness', async () => {
  let published;
  const h = harness(async (url, options) => {
    if (url.endsWith('/focus_status')) published = JSON.parse(options.body);
    return {ok:true,json:async()=>({command:null,focus:null})};
  });
  await h.poll();
  Object.assign(h.stored,{port:38547,token:'fixture'});
  h.chrome.permissions.contains = async () => false;
  const result = await h.poll();
  assert.equal(result.connected,true);
  assert.equal(result.totalFocusAvailable,false);
  assert.match(result.error,/access to all websites/);
  assert.equal(published.available,false);
  h.chrome.permissions.contains = async () => true;
  assert.equal((await h.poll()).totalFocusAvailable,true);
});

test('a rule-installation failure acknowledges the command and recovers on retry', async () => {
  let fail = true;
  const messages = [];
  const h = harness(async (url, options) => {
    if(options?.body) messages.push({url,body:JSON.parse(options.body)});
    return {ok:true,json:async()=>({command:{id:'start',name:'browser.focus_status',arguments:{}},focus:policy()})};
  });
  await h.poll();
  Object.assign(h.stored,{port:38547,token:'fixture'});
  const update = h.chrome.declarativeNetRequest.updateDynamicRules;
  h.chrome.declarativeNetRequest.updateDynamicRules = async change => {
    if(fail) throw new Error('Site access was denied');
    return update(change);
  };
  const result = await h.poll();
  assert.equal(result.connected,true);
  assert.equal(result.totalFocusAvailable,false);
  const failure = messages.find(item=>item.url.endsWith('/result')).body;
  assert.equal(failure.ok,false);
  assert.match(failure.error,/Site access was denied/);
  assert.equal(h.stored.focus,undefined);
  fail=false;messages.length=0;
  assert.equal((await h.poll()).totalFocusAvailable,true);
  assert.equal(messages.find(item=>item.url.endsWith('/result')).body.result.applied,true);
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
