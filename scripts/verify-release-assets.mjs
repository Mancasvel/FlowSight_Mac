import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (name) => readFileSync(join(root, name), 'utf8');
const config = JSON.parse(read('apps/agent/src-tauri/tauri.conf.json'));
const expected = (process.env.RELEASE_TAG || process.env.TAG || `v${config.version}`).replace(/^v/, '');
for (const name of ['package.json', 'apps/agent/package.json', 'apps/agent/src-tauri/tauri.conf.json']) {
  assert.equal(JSON.parse(read(name)).version, expected, `${name} must match release tag`);
}
assert.equal(read('apps/agent/src-tauri/Cargo.toml').match(/^version = "([^"]+)"/m)?.[1], expected);
assert.equal(read('apps/agent/src-tauri/Cargo.lock').match(/name = "app"\r?\nversion = "([^"]+)"/)?.[1], expected);
const files = ['manifest.json', 'worker.js', 'options.html', 'options.js', 'blocked.html', 'blocked.js', 'focus.css', 'icon16.png', 'icon48.png', 'icon128.png'];
const source = join(root, 'apps/agent/browser-extension');
assert.equal(JSON.parse(readFileSync(join(source, 'manifest.json'), 'utf8')).version, '1.1.1');
for (const name of files) {
  assert.equal(config.bundle.resources[`../browser-extension/${name}`], `browser-extension/${name}`, `Missing packaged ${name}`);
  assert.ok(readFileSync(join(source, name)).length, `${name} must not be empty`);
}
if (process.argv[2]) {
  const found = [];
  function visit(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const child = join(dir, entry.name);
      if (entry.name === 'browser-extension') found.push(child);
      else visit(child);
    }
  }
  visit(resolve(process.argv[2]));
  assert.equal(found.length, 1, 'Expected one packaged browser-extension directory');
  const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
  for (const name of files) assert.equal(digest(join(found[0], name)), digest(join(source, name)), `Packaged ${name} differs from source`);
  console.log(`Verified extension 1.1.1 in ${found[0]}`);
}
console.log(`Release ${expected}: five version manifests and ten extension resources verified.`);
