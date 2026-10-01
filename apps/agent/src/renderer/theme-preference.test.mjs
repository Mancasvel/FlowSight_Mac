import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./public/theme-preference.js', import.meta.url), 'utf8');

function createThemePage({ saved = null, systemDark = false, storageAvailable = true } = {}) {
  const values = new Map(saved ? [['flowsight_theme_preference', saved]] : []);
  const root = { dataset: {}, style: {} };
  const sheets = [
    { media: '(prefers-color-scheme: dark)', background: 'dark' },
    { media: '(prefers-color-scheme: dark)', background: 'dark' },
  ];
  // Vite injects its bundled light CSS after the links present in index.html.
  const stylesheetOrder = [...sheets, { media: '', background: 'light' }];
  const button = {
    title: 'Switch to dark mode',
    attributes: {},
    setAttribute(name, value) { this.attributes[name] = value; },
    addEventListener(type, listener) { if (type === 'click') this.click = listener; },
  };
  let onReady;
  let onSystemChange;
  const document = {
    documentElement: root,
    readyState: 'loading',
    head: {
      appendChild(sheet) {
        stylesheetOrder.splice(stylesheetOrder.indexOf(sheet), 1);
        stylesheetOrder.push(sheet);
      },
    },
    getElementById(id) {
      return { themeDarkBase: sheets[0], themeDarkMobile: sheets[1], themeToggleBtn: button }[id];
    },
    addEventListener(type, listener) { if (type === 'DOMContentLoaded') onReady = listener; },
  };
  const system = {
    matches: systemDark,
    addEventListener(type, listener) { if (type === 'change') onSystemChange = listener; },
  };
  const window = {
    matchMedia: () => system,
    localStorage: {
      getItem(key) {
        if (!storageAvailable) throw new Error('Storage unavailable');
        return values.get(key) ?? null;
      },
      setItem(key, value) {
        if (!storageAvailable) throw new Error('Storage unavailable');
        values.set(key, value);
      },
    },
  };

  runInNewContext(source, { document, window });
  onReady();
  return {
    root, sheets, button, values,
    visibleBackground() {
      return stylesheetOrder.filter((sheet) => !sheet.media || sheet.media === 'all').at(-1)?.background;
    },
    systemChange(matches) { onSystemChange({ matches }); },
  };
}

test('theme button switches the entire stylesheet and persists the manual choice', () => {
  const page = createThemePage({ systemDark: true });
  assert.equal(page.root.dataset.theme, 'dark');
  assert.deepEqual(page.sheets.map((sheet) => sheet.media), ['all', 'all']);
  assert.equal(page.button.attributes['aria-pressed'], 'true');
  assert.equal(page.visibleBackground(), 'dark');

  page.button.click();
  assert.equal(page.root.dataset.theme, 'light');
  assert.deepEqual(page.sheets.map((sheet) => sheet.media), ['not all', 'not all']);
  assert.equal(page.visibleBackground(), 'light');
  assert.equal(page.button.title, 'Switch to dark mode');
  assert.equal(page.values.get('flowsight_theme_preference'), 'light');
  page.systemChange(true);
  assert.equal(page.root.dataset.theme, 'light');

  const reopened = createThemePage({ saved: 'light', systemDark: true });
  assert.equal(reopened.root.dataset.theme, 'light');
  reopened.button.click();
  assert.equal(reopened.root.dataset.theme, 'dark');
  assert.equal(reopened.visibleBackground(), 'dark');
  assert.equal(reopened.button.title, 'Switch to light mode');
});

test('theme follows system changes until the user chooses a mode', () => {
  const page = createThemePage();
  assert.equal(page.root.dataset.theme, 'light');
  page.systemChange(true);
  assert.equal(page.root.dataset.theme, 'dark');
  page.button.click();
  page.systemChange(true);
  assert.equal(page.root.dataset.theme, 'light');
});

test('theme switching remains available when local storage is blocked', () => {
  const page = createThemePage({ storageAvailable: false });
  page.button.click();
  assert.equal(page.root.dataset.theme, 'dark');
  assert.equal(page.root.style.colorScheme, 'dark');
});
