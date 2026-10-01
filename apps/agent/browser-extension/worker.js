// The native app is the authority. The extension performs only commands that
// arrive through the loopback bridge after the user confirms them in FlowSight.
const POLL_ALARM = 'flowsight-poll';
const FOCUS_ALARM = 'flowsight-focus-expiry';
const FOCUS_RULE_IDS = Array.from({ length: 40 }, (_, index) => 1000 + index);
let polling = false;
let focusQueue = Promise.resolve();
const focusMutation = run => {
  const result = focusQueue.then(run);
  focusQueue = result.catch(() => {});
  return result;
};

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function ruleFor(pattern, id) {
  if (typeof pattern !== 'string' || pattern.length > 240 || /\s/.test(pattern)) {
    throw new Error('Use a domain or HTTP(S) URL path without spaces.');
  }
  if (pattern.includes('://') && !/^https?:\/\//i.test(pattern)) {
    throw new Error('Only HTTP(S) domains and paths can be blocked.');
  }
  const url = new URL(/^https?:\/\//i.test(pattern) ? pattern : `https://${pattern}`);
  if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash) {
    throw new Error('Only HTTP(S) domains and paths can be blocked.');
  }
  const host = escapeRegex(url.hostname);
  const includeSubdomains = !url.hostname.startsWith('www.');
  const domain = `${includeSubdomains ? '(?:[^/]+\\.)?' : ''}${host}`;
  const path = url.pathname === '/' ? '(?:[/?#]|$)'
    : `${escapeRegex(url.pathname)}${url.pathname.endsWith('/') ? '' : '(?:[/?#]|$)'}`;
  return {
    id,
    priority: 1,
    action: { type: 'block' },
    condition: { regexFilter: `^https?://${domain}(?::[0-9]+)?${path}`, resourceTypes: ['main_frame'] },
  };
}

async function expireBlocks() {
  const { focus } = await chrome.storage.local.get('focus');
  if (focus && Date.parse(focus.expiresAt) <= Date.now()) await focusMutation(async () => {
    const current = (await chrome.storage.local.get('focus')).focus;
    if (current && Date.parse(current.expiresAt) <= Date.now()) await clearFocus();
  });
  const { blocks = [] } = await chrome.storage.local.get('blocks');
  const now = Date.now();
  const expired = blocks.filter((block) => block.expiresAt <= now);
  if (!expired.length) return;
  await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: expired.map((block) => block.id) });
  await chrome.storage.local.set({ blocks: blocks.filter((block) => block.expiresAt > now) });
}

function focusPattern(pattern) {
  const rule = ruleFor(pattern, 1);
  const url = new URL(/^https?:\/\//i.test(pattern) ? pattern : `https://${pattern}`);
  if (url.port || !url.hostname.includes('.') || url.hostname.endsWith('.') || /^\d+\.\d+\.\d+\.\d+$/.test(url.hostname)) {
    throw new Error('Use a public domain or HTTP(S) path without a port.');
  }
  return rule.condition;
}

async function clearFocus() {
  await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: FOCUS_RULE_IDS });
  await chrome.storage.local.set({ focus: null });
  await chrome.alarms.clear(FOCUS_ALARM);
}

async function reconcileFocus(policy) {
  return focusMutation(() => applyFocusPolicy(policy));
}

async function applyFocusPolicy(policy) {
  const { focus, cancelledFocusId } = await chrome.storage.local.get(['focus', 'cancelledFocusId']);
  if (!policy || policy.id === cancelledFocusId || Date.parse(policy.expiresAt) <= Date.now()) {
    if (focus) await clearFocus();
    if (!policy && cancelledFocusId) await chrome.storage.local.set({ cancelledFocusId: null });
    return;
  }
  const expiresAt = Date.parse(policy.expiresAt);
  if (!policy.id || !Number.isFinite(expiresAt) || expiresAt > Date.now() + 181 * 60000
      || !Array.isArray(policy.patterns) || !policy.patterns.length || policy.patterns.length > 20
      || !Array.isArray(policy.exceptions) || policy.exceptions.length > 20) throw new Error('Invalid total focus policy.');
  const rules = [
    ...policy.patterns.map((pattern, i) => ({ id: 1000 + i, priority: 10,
      action: { type: 'redirect', redirect: { extensionPath: '/blocked.html' } }, condition: focusPattern(pattern) })),
    ...policy.exceptions.map((pattern, i) => ({ id: 1020 + i, priority: 100,
      action: { type: 'allow' }, condition: focusPattern(pattern) })),
  ];
  const installed = (await chrome.declarativeNetRequest.getDynamicRules()).filter(rule => FOCUS_RULE_IDS.includes(rule.id));
  const changed = JSON.stringify(focus) !== JSON.stringify(policy)
    || installed.length !== rules.length || !rules.every(rule => installed.some(item => item.id === rule.id
      && item.priority === rule.priority && item.action.type === rule.action.type
      && item.condition.regexFilter === rule.condition.regexFilter));
  if (changed) {
    await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: FOCUS_RULE_IDS, addRules: rules });
    await chrome.storage.local.set({ focus: policy });
    await chrome.alarms.create(FOCUS_ALARM, { when: expiresAt });
    // Replace already open distractions with a local page. No URLs or tab titles leave the browser.
    const tabs = await chrome.tabs.query({});
    for (const tab of tabs) {
      if (!/^https?:\/\//i.test(tab.url || '')) continue;
      const blocked = rules.some(rule => rule.action.type === 'redirect' && new RegExp(rule.condition.regexFilter, 'i').test(tab.url));
      const allowed = rules.some(rule => rule.action.type === 'allow' && new RegExp(rule.condition.regexFilter, 'i').test(tab.url));
      if (blocked && !allowed) await chrome.tabs.update(tab.id, { url: chrome.runtime.getURL('blocked.html') }).catch(() => {});
    }
  }
}

async function focusStatus() {
  await expireBlocks();
  const { focus, cancelledFocusId } = await chrome.storage.local.get(['focus', 'cancelledFocusId']);
  const installed = await chrome.declarativeNetRequest.getDynamicRules();
  return { sessionId: focus?.id || null, applied: Boolean(focus && installed.some(rule => rule.id === 1000)),
    cancelledSessionId: cancelledFocusId || null };
}

async function cancelFocus() {
  return focusMutation(async () => {
    const { focus } = await chrome.storage.local.get('focus');
    if (focus) await chrome.storage.local.set({ cancelledFocusId: focus.id });
    await clearFocus();
    return { ended: true };
  });
}

async function publishFocusStatus(port, token) {
  const response = await fetch(`http://127.0.0.1:${port}/focus_status`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'X-FlowSight-Token': token },
    body: JSON.stringify(await focusStatus()), signal: AbortSignal.timeout(8000),
  });
  if (!response.ok) throw new Error('FlowSight did not accept browser status.');
}

async function releaseBlocksAfterDisconnect() {
  const { lastConnectedAt = 0, blocks = [] } = await chrome.storage.local.get(['lastConnectedAt', 'blocks']);
  if (!blocks.length || Date.now() - lastConnectedAt < 75000) return;
  await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: blocks.map((block) => block.id) });
  await chrome.storage.local.set({ blocks: [] });
}

async function runCommand(name, args) {
  if (name === 'browser.focus_status') return focusStatus();
  if (name === 'browser.unblock_all') {
    const { blocks = [] } = await chrome.storage.local.get('blocks');
    if (blocks.length) {
      await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: blocks.map((block) => block.id) });
      await chrome.storage.local.set({ blocks: [] });
    }
    return { unblockedAll: true };
  }
  if (name === 'browser.list_tabs') {
    const tabs = await chrome.tabs.query({});
    const { closedTabs = [] } = await chrome.storage.local.get('closedTabs');
    return {
      tabs: tabs.filter((tab) => /^https?:\/\//i.test(tab.url || '')).map((tab) => ({
        id: tab.id, title: tab.title || '', url: tab.url, active: tab.active,
      })),
      restorable: closedTabs.map((tab) => ({ restoreId: tab.restoreId, title: tab.title, url: tab.url })),
    };
  }
  if (name === 'browser.block') {
    const { blocks = [], nextRuleId = 10000 } = await chrome.storage.local.get(['blocks', 'nextRuleId']);
    const expiresAt = Date.now() + args.duration_minutes * 60000;
    const incoming = [...new Set(args.patterns)];
    const replaced = blocks.filter((block) => incoming.includes(block.pattern));
    const kept = blocks.filter((block) => !incoming.includes(block.pattern));
    const additions = incoming.map((pattern, index) => ({ pattern, id: nextRuleId + index, expiresAt }));
    const addRules = additions.map((block) => ruleFor(block.pattern, block.id));
    await chrome.declarativeNetRequest.updateDynamicRules({
      removeRuleIds: replaced.map((block) => block.id), addRules,
    });
    await chrome.storage.local.set({ blocks: [...kept, ...additions], nextRuleId: nextRuleId + additions.length });
    return { blocked: incoming, expiresAt: new Date(expiresAt).toISOString() };
  }
  if (name === 'browser.unblock') {
    const { blocks = [] } = await chrome.storage.local.get('blocks');
    const removed = blocks.filter((block) => args.patterns.includes(block.pattern));
    await chrome.declarativeNetRequest.updateDynamicRules({ removeRuleIds: removed.map((block) => block.id) });
    await chrome.storage.local.set({ blocks: blocks.filter((block) => !args.patterns.includes(block.pattern)) });
    return { unblocked: removed.map((block) => block.pattern) };
  }
  if (name === 'browser.close_tab') {
    const tab = await chrome.tabs.get(args.tab_id);
    if (!/^https?:\/\//i.test(tab.url || '')) throw new Error('Only ordinary web pages can be closed.');
    const restoreId = crypto.randomUUID();
    const { closedTabs = [] } = await chrome.storage.local.get('closedTabs');
    const record = { restoreId, url: tab.url, title: tab.title || tab.url, windowId: tab.windowId, index: tab.index };
    await chrome.storage.local.set({ closedTabs: [...closedTabs, record].slice(-50) });
    await chrome.tabs.remove(tab.id);
    return { closed: record };
  }
  if (name === 'browser.restore_tab') {
    const { closedTabs = [] } = await chrome.storage.local.get('closedTabs');
    const tab = closedTabs.find((item) => item.restoreId === args.restore_id);
    if (!tab) throw new Error('That restore record was not found.');
    let created;
    try {
      created = await chrome.tabs.create({ url: tab.url, windowId: tab.windowId, index: tab.index, active: true });
    } catch (_) {
      created = await chrome.tabs.create({ url: tab.url, active: true });
    }
    await chrome.storage.local.set({ closedTabs: closedTabs.filter((item) => item.restoreId !== args.restore_id) });
    return { restoredTabId: created.id, url: tab.url };
  }
  throw new Error('Unsupported browser command.');
}

async function flushResults(port, token) {
  const { pendingResults = [] } = await chrome.storage.local.get('pendingResults');
  const remaining = [];
  for (const result of pendingResults) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/result`, {
        method: 'POST', headers: { 'Content-Type': 'application/json', 'X-FlowSight-Token': token },
        body: JSON.stringify(result), signal: AbortSignal.timeout(8000),
      });
      if (!response.ok && response.status !== 404) remaining.push(result);
    } catch (_) {
      remaining.push(result);
    }
  }
  if (remaining.length !== pendingResults.length) await chrome.storage.local.set({ pendingResults: remaining });
}

async function poll() {
  if (polling) return;
  polling = true;
  try {
    await expireBlocks();
    const { port, token } = await chrome.storage.local.get(['port', 'token']);
    if (!port || !token) { await releaseBlocksAfterDisconnect(); return; }
    await flushResults(port, token);
    const response = await fetch(`http://127.0.0.1:${port}/next`, {
      headers: { 'X-FlowSight-Token': token }, signal: AbortSignal.timeout(8000),
    });
    if (!response.ok) { await releaseBlocksAfterDisconnect(); return; }
    await chrome.storage.local.set({ lastConnectedAt: Date.now() });
    const { command, focus = null } = await response.json();
    await reconcileFocus(focus);
    await publishFocusStatus(port, token);
    if (!command) return;
    let result;
    try {
      result = { id: command.id, ok: true, result: await runCommand(command.name, command.arguments) };
    } catch (error) {
      result = { id: command.id, ok: false, error: String(error.message || error) };
    }
    const { pendingResults = [] } = await chrome.storage.local.get('pendingResults');
    await chrome.storage.local.set({ pendingResults: [...pendingResults, result].slice(-20) });
    await flushResults(port, token);
  } catch (_) {
    // FlowSight may be closed. Never keep a site blocked indefinitely.
    await releaseBlocksAfterDisconnect().catch(() => {});
  } finally {
    polling = false;
  }
}

chrome.runtime.onInstalled.addListener(({ reason }) => {
  chrome.alarms.create(POLL_ALARM, { periodInMinutes: 0.5 });
  if (reason === 'install') chrome.runtime.openOptionsPage();
  poll();
});
chrome.runtime.onStartup.addListener(() => { chrome.alarms.create(POLL_ALARM, { periodInMinutes: 0.5 }); poll(); });
chrome.action.onClicked.addListener(() => chrome.runtime.openOptionsPage());
chrome.alarms.onAlarm.addListener((alarm) => { if (alarm.name === POLL_ALARM || alarm.name === FOCUS_ALARM) poll(); });
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message?.type === 'poll-now') poll();
  if (message?.type === 'end-focus' && sender.url?.startsWith(chrome.runtime.getURL(''))) {
    cancelFocus().then(result => { reply(result); poll(); }, error => reply({ error: error.message }));
    return true;
  }
});
chrome.alarms.create(POLL_ALARM, { periodInMinutes: 0.5 });
poll();
