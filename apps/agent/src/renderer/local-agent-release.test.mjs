import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const markup = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const styles = readFileSync(new URL('./local-agent.css', import.meta.url), 'utf8');

test('the local action agent is configured in Settings, without a chat tab or unverified token controls', () => {
  assert.match(markup, /id="localAgentSettingsCard"/);
  assert.match(markup, /id="localAgentManageDetails"/);
  assert.match(markup, /id="localAgentProposal"[^>]*hidden/);
  assert.match(markup, /id="localAgentInstallChromeBtn"/);
  assert.match(markup, /id="localAgentInstallEdgeBtn"/);
  assert.match(markup, /id="localAgentOpenExtensionBtn"/);
  assert.match(markup, /open_browser_extension_folder/);
  assert.deepEqual(
    [...markup.matchAll(/<button class="nav-item[^"]*" data-tab="([^"]+)"/g)].map((match) => match[1]),
    ['tabOnline', 'tabSummary', 'tabToday', 'tabCloudInsights', 'tabProfile'],
  );
  assert.doesNotMatch(markup, /id="navLocalAgent"|id="tabLocalAgent"|id="localAgentForm"/);
  assert.match(markup, /id="navCloudInsights"/);
  assert.match(markup, /id="tabCloudInsights"/);
  assert.match(markup, /id="coachComposerForm"/);
  assert.match(markup, /send_coach_chat_message/);
  assert.doesNotMatch(markup, /id="localAgentExternalConnections"|id="localAgentToken"/);
  assert.match(markup, /listen\('local-agent-digest'/);
  assert.match(styles, /\.local-agent-proposal\[hidden\]\s*\{\s*display:\s*none/);
});

test('browser setup is a three-step store, pair, and verify flow with a masked key', () => {
  assert.match(markup, /id="localAgentBrowserSetup"/);
  assert.match(markup, /Install Browser Controls/);
  assert.match(markup, /Open Chrome Web Store/);
  assert.match(markup, /Pair on this computer/);
  assert.match(markup, /id="localAgentBrowserToken" type="password" readonly/);
  assert.match(markup, /Check the connection/);
  assert.match(markup, /id="localAgentCheckBrowserBtn"/);
  assert.match(markup, /invoke\('open_browser_extension_store', \{ browser \}\)/);
  assert.match(markup, /const key = localBrowserPairing\?\.token/);
  assert.doesNotMatch(markup, /localAgentBrowserToken'\)\.textContent\s*=\s*browser\?\.token/);
  assert.match(styles, /\.local-agent-browser-badge\[data-state="connected"\]/);
});
