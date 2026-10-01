import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const html = await readFile(new URL('./index.html', import.meta.url), 'utf8');
const theme = await readFile(new URL('./mobile-theme.css', import.meta.url), 'utf8');

test('PDF success dialog uses the shared, touch-sized actions', () => {
  assert.match(html, /class="modal-content pdf-download-dialog" role="dialog" aria-modal="true"/);
  assert.match(html, /class="button button-secondary" id="pdfDownloadOpenFolderBtn">Open folder<\/button>/);
  assert.match(html, /class="button button-primary" id="pdfDownloadOkBtn">Done<\/button>/);
  assert.match(theme, /\.pdf-download-dialog__actions \.button \{ min-height: 44px;/);
  assert.match(html, /e\.key === 'Escape'/);
  assert.match(html, /previousFocus\?\.isConnected/);
});

test('PDF dialog shows the actual saved name and opens its containing folder', () => {
  assert.match(html, /const displayName = savedPath\.split\(/);
  assert.match(html, /invoke\('open_path_in_file_manager', \{ path: folderPath \}\)/);
});
