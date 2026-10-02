import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./options.js', import.meta.url), 'utf8');
const manifest = JSON.parse(readFileSync(new URL('./manifest.json', import.meta.url), 'utf8'));

function page(chrome) {
  const handlers = new Map();
  const submitButton = { disabled: false };
  const elements = {
    port: { value: '' },
    token: { value: '' },
    status: { textContent: '' },
    pairForm: {
      querySelector: () => submitButton,
      addEventListener: (name, handler) => handlers.set(name, handler),
    },
  };
  const sandbox = {
    document: { getElementById: (id) => elements[id] },
    ...(chrome ? { chrome } : {}),
  };
  runInNewContext(source, sandbox);
  return { elements, handlers, submitButton };
}

test('opening options.html as a file explains how to load the extension instead of crashing', () => {
  const { elements, handlers, submitButton } = page();
  assert.match(elements.status.textContent, /not running as an installed browser extension/);
  assert.match(elements.status.textContent, /official store link/);
  assert.equal(submitButton.disabled, true);
  assert.equal(handlers.has('submit'), false);
});

test('installed extension loads and saves pairing details', async () => {
  let saved;
  let message;
  const chrome = {
    storage: {
      local: {
        get: async () => ({ port: 38547, token: 'previous-key' }),
        set: async (value) => { saved = value; },
      },
    },
    runtime: { sendMessage: async (value) => { message = value; return {connected: true}; } },
  };
  const { elements, handlers, submitButton } = page(chrome);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(elements.port.value, '38547');
  assert.equal(elements.token.value, 'previous-key');
  assert.equal(submitButton.disabled, false);
  elements.token.value = 'new-key';
  await handlers.get('submit')({ preventDefault() {} });
  assert.equal(saved.port, 38547);
  assert.equal(saved.token, 'new-key');
  assert.equal(message.type, 'poll-now');
  assert.match(elements.status.textContent, /Connected to FlowSight/);
});

test('store package has its runtime files and opens pairing options from the toolbar', () => {
  assert.equal(manifest.manifest_version, 3);
  assert.equal(manifest.options_page, 'options.html');
  assert.equal(manifest.action.default_title, 'FlowSight Browser Controls');
  assert.deepEqual(manifest.host_permissions, ['http://127.0.0.1/*','http://*/*','https://*/*']);
  assert.equal(manifest.web_accessible_resources[0].resources[0], 'blocked.html');
  for (const name of ['manifest.json', 'worker.js', 'options.html', 'options.js', 'blocked.html', 'blocked.js', 'focus.css', ...Object.values(manifest.icons)]) {
    assert.ok(statSync(new URL(name, import.meta.url)).size > 0, `${name} is missing or empty`);
  }
  const worker = readFileSync(new URL('./worker.js', import.meta.url), 'utf8');
  assert.match(worker, /reason === 'install'\) chrome\.runtime\.openOptionsPage\(\)/);
  assert.match(worker, /chrome\.action\.onClicked\.addListener/);
});

test('pairing failure is displayed without claiming success', async () => {
  const {elements, handlers} = page({storage: {local: {get: async () => ({}), set: async () => {}}},
    runtime: {sendMessage: async () => ({connected: false, error: 'The pairing key was rejected.'})}});
  elements.token.value = 'invalid';
  await handlers.get('submit')({preventDefault() {}});
  assert.match(elements.status.textContent, /key was rejected/);
  assert.doesNotMatch(elements.status.textContent, /Connected to FlowSight/);
});
