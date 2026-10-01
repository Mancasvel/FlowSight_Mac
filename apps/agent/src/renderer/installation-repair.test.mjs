import { setLanguagePreference } from './i18n.mjs';
setLanguagePreference('en',{persist:false});
import assert from 'node:assert/strict';
import test from 'node:test';
import { createInstallationRepairController } from './installation-repair.mjs';

function harness({ healthy = true, microsoftStoreBuild = false } = {}) {
  let overlay = null;
  let repairs = 0;
  const events = new Map();
  const nodes = new Map();
  function node(id) {
    const listeners = new Map();
    return {
      id, hidden: false, textContent: '', innerHTML: '', style: {},
      attributes: new Map(),
      addEventListener(type, listener) { listeners.set(type, listener); },
      setAttribute(name, value) { this.attributes.set(name, value); },
      fire(type, event = {}) { return listeners.get(type)?.(event); },
      focus() {},
      remove() { overlay = null; },
      querySelector(selector) {
        if (!nodes.has(selector)) nodes.set(selector, node(selector));
        return nodes.get(selector);
      },
    };
  }
  const document = {
    getElementById(id) { return id === 'repairOverlay' ? overlay : null; },
    createElement() { return node('repairOverlay'); },
    body: { appendChild(element) { overlay = element; } },
  };
  const controller = createInstallationRepairController({
    document,
    isMicrosoftStoreBuild: microsoftStoreBuild,
    invoke: async command => {
      if (command === 'check_installation_health') return { healthy };
      assert.equal(command, 'repair_installation');
      repairs++;
      throw new Error('private installer path C:\\sensitive\\repair.exe');
    },
    listen: async (event, callback) => {
      events.set(event, callback);
      return () => events.delete(event);
    },
  });
  return { controller, events, nodes, get overlay() { return overlay; }, get repairs() { return repairs; } };
}

const flush = () => new Promise(resolve => setImmediate(resolve));

test('healthy installation does not enter repair', async () => {
  const app = harness();
  assert.equal(await app.controller.check(), true);
  assert.equal(app.overlay, null);
  assert.equal(app.repairs, 0);
});

test('damaged installation starts a signed repair and keeps technical failures out of the UI', async () => {
  const app = harness({ healthy: false });
  assert.equal(await app.controller.check(), false);
  assert.equal(app.controller.needsRepair(), true);
  assert.ok(app.overlay);
  assert.match(app.overlay.innerHTML, /Your personal data stays in place/);
  await flush();
  assert.equal(app.repairs, 1);
  assert.equal(app.nodes.get('#repairError').hidden, false);
  assert.match(app.nodes.get('#repairError').textContent, /Check your connection and try again/);
  assert.doesNotMatch(app.nodes.get('#repairError').textContent, /sensitive|repair\.exe/);
  await app.nodes.get('#repairRetryBtn').fire('click');
  await flush();
  assert.equal(app.repairs, 2);
  app.nodes.get('#repairCloseBtn').fire('click');
  app.controller.show();
  await flush();
  assert.equal(app.repairs, 2, 'reopening does not start another automatic download');
});

test('Microsoft Store copy gives Store repair guidance without launching the GitHub installer', async () => {
  const app = harness({ healthy: false, microsoftStoreBuild: true });
  assert.equal(await app.controller.check(), false);
  await flush();
  assert.match(app.overlay.innerHTML, /Microsoft Store Library/);
  assert.equal(app.repairs, 0);
});
