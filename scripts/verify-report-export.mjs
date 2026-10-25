// Portable production-renderer E2E with explicitly synthetic native IPC.
// Exercises generation, a private saved choice, and the real PDF export button.
import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { resolve, extname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';
import { chromium, webkit } from 'playwright';
import { syntheticWorkReport, installSyntheticWorkReportNative } from './fixtures/work-report.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const output = resolve(process.env.FLOWSIGHT_REPORT_EVIDENCE || '.impeccable/review/report-export');
await mkdir(output, { recursive: true });
let server;
let rendererUrl = process.env.FLOWSIGHT_RENDERER_URL;
if (!rendererUrl) {
  const dist = resolve(process.env.FLOWSIGHT_RENDERER_DIST || resolve(root, 'apps/agent/dist'));
  const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.woff2': 'font/woff2', '.woff': 'font/woff', '.svg': 'image/svg+xml' };
  server = createServer(async (request, response) => {
    const relative = decodeURIComponent(new URL(request.url, 'http://localhost').pathname).replace(/^\/+/, '') || 'index.html';
    const path = resolve(dist, relative);
    if (!path.startsWith(dist + sep)) { response.writeHead(403).end(); return; }
    try { response.writeHead(200, { 'Content-Type': mime[extname(path)] || 'application/octet-stream' }); response.end(await readFile(path)); }
    catch { response.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  rendererUrl = `http://127.0.0.1:${server.address().port}`;
}
const origin = new URL(rendererUrl).origin;
let browser;
const results = [];

function pdfPages(bytes) {
  const source = Buffer.from(bytes).toString('latin1');
  assert.ok(source.startsWith('%PDF-'), 'The UI must emit a PDF document, not arbitrary bytes.');
  const objects = new Map([...source.matchAll(/(\d+) 0 obj\s*([\s\S]*?)endobj/g)].map(match => [match[1], match[2]]));
  return [...objects.values()].filter(object => /\/Type\s*\/Page\b/.test(object)).map(page => {
    const reference = page.match(/\/Contents\s+(\d+) 0 R/);
    assert.ok(reference, 'Each PDF page needs a content stream.');
    const object = objects.get(reference[1]);
    const stream = object.match(/stream\r?\n([\s\S]*?)\r?\nendstream/);
    assert.ok(stream, 'Missing PDF content stream.');
    return /\/FlateDecode/.test(object) ? inflateSync(Buffer.from(stream[1], 'latin1')).toString('latin1') : stream[1];
  });
}

try {
  const browserType = process.env.FLOWSIGHT_E2E_BROWSER === 'webkit' ? webkit : chromium;
  browser = await browserType.launch({ headless: true });
  for (const view of [
    { name: 'wide-en', width: 900, height: 1000, language: 'en', theme: 'light' },
    { name: 'compact-es', width: 370, height: 900, language: 'es', theme: 'light' },
    { name: 'wide-dark', width: 900, height: 1000, language: 'en', theme: 'dark' },
  ]) {
    const context = await browser.newContext({ viewport: { width: view.width, height: view.height },
      locale: view.language === 'es' ? 'es-ES' : 'en-GB', timezoneId: 'Europe/Madrid', colorScheme: view.theme, reducedMotion: 'reduce' });
    const externalRequests = [];
    await context.route('**/*', route => {
      if (new URL(route.request().url()).origin === origin) return route.continue();
      externalRequests.push(route.request().url()); return route.abort();
    });
    await context.addInitScript(installSyntheticWorkReportNative, { language: view.language, report: syntheticWorkReport });
    const page = await context.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(rendererUrl, { waitUntil: 'networkidle' });
    await page.locator('.nav-item[data-tab="tabSummary"]').click();
    assert.equal(await page.locator('#workReportPeriod').inputValue(), '7');
    await page.locator('#generateReportBtn').click();
    await page.locator('[data-review-suggestion="0"]').waitFor();
    assert.equal(await page.evaluate(() => window.reportCalls.filter(call => call.command === 'generate_local_status_report').at(-1).args.periodDays), 7);
    assert.equal(await page.locator('.sr-grade-number strong').innerText(), view.language === 'es' ? '6,9' : '6.9');
    assert.equal(await page.locator('.sr-day').count(), 7);
    assert.equal(await page.locator('.sr-category-row').count(), 6);
    assert.equal(await page.locator('.sr-category-row').last().locator('.sr-category-name').innerText(), view.language === 'es' ? 'Otras categorías' : 'Other categories');
    assert.equal(await page.locator('.sr-report-details').evaluate(node => node.open), false);
    assert.equal(await page.locator('[data-review-suggestion]').count(), 3);
    assert.ok(await page.locator('#srEvidenceTitle').evaluate(node => !!(node.compareDocumentPosition(document.querySelector('#srNextTitle')) & Node.DOCUMENT_POSITION_FOLLOWING)));
    assert.ok(await page.locator('#statusReportModalBody').evaluate(node => node.scrollWidth <= node.clientWidth + 1));
    await page.locator('.sr-report-details > summary').click();
    assert.ok(await page.locator('.sr-report-details').evaluate(node => node.open));
    assert.match(await page.locator('.sr-report-details').innerText(), /RAW_SYNTHETIC_SCREEN_CONTEXT/);
    await page.locator('.sr-report-details > summary').click();
    await page.locator('.sr-section').filter({ has: page.locator('#srEvidenceTitle') }).screenshot({ path: resolve(output, `${view.name}-charts.png`) });

    // Private decisions persist through the real form, yet stay out of export.
    await page.locator('[data-review-suggestion="0"]').click();
    await page.locator('#reviewDecisionText').fill('SYNTHETIC_PRIVATE_REVIEW_CHOICE');
    await page.locator('#reviewDecisionDate').fill('2026-10-26');
    await page.locator('#reviewDecisionForm button[type="submit"]').click();
    await page.locator('[data-review-edit]').waitFor({ state: 'attached' });
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('synthetic-report-decisions'))[0].text), 'SYNTHETIC_PRIVATE_REVIEW_CHOICE');
    const button = page.locator('#downloadReportPdfBtn');
    if (view.name === 'wide-en') {
      await page.evaluate(() => { window.reportFailExport = true; });
      await button.click();
      await page.waitForFunction(() => window.reportExports.at(-1)?.status === 'failed');
      await page.locator('.toast').filter({ hasText: /PDF export failed/ }).waitFor();
      assert.equal(await button.isEnabled(), true, 'A failed export must allow retry.');
      assert.equal(await page.locator('#pdfDownloadOverlay').count(), 0);
    }
    await page.evaluate(() => { window.reportFailExport = false; window.reportHoldExport = true; });
    await button.click();
    await page.waitForFunction(() => typeof window.releaseReportExport === 'function');
    assert.equal(await button.isDisabled(), true, 'Export stays disabled until native saving finishes.');
    await page.evaluate(() => window.releaseReportExport());
    await page.locator('#pdfDownloadOverlay').waitFor();
    assert.equal(await button.isEnabled(), true);
    assert.match(await page.locator('.pdf-download-filename').innerText(), /^FlowSight_Work-Report_2026-10-12_to_2026-10-18\.pdf$/);
    assert.equal(await page.locator('.pdf-download-dialog__location').innerText(), '/synthetic/downloads');
    const exported = await page.evaluate(() => window.reportExports.findLast(item => item.status === 'saved'));
    assert.equal(exported.synthetic, true);
    const pages = pdfPages(exported.bytes);
    assert.equal(pages.length, 3, 'The actual PDF export button must emit the three-page review.');
    assert.ok(pages[0].includes(view.language === 'es' ? 'Actividad por d\xeda' : 'Activity by day'));
    assert.ok(pages[0].includes(view.language === 'es' ? 'Tiempo por categor\xeda' : 'Time by category'));
    assert.ok(pages[1].includes(view.language === 'es' ? 'La comparaci\xf3n detr\xe1s de tu nota' : 'The comparison behind your grade'));
    assert.ok(pages[2].includes(view.language === 'es' ? 'Qu\xe9 hacer ahora' : 'What to do next'));
    const pdfText = pages.join('\n');
    for (const marker of ['SYNTHETIC_PRIVATE_REVIEW_CHOICE', 'SYNTHETIC_PRIVATE_PAYLOAD_DECISION', 'RAW_SYNTHETIC_SCREEN_CONTEXT',
      'RAW_SYNTHETIC_RECOMMENDATION', 'Synthetic work tool']) assert.ok(!pdfText.includes(marker), `${marker} leaked into PDF export.`);
    await writeFile(resolve(output, `${view.name}-synthetic-export.pdf`), Buffer.from(exported.bytes));
    await page.locator('#pdfDownloadOverlay').screenshot({ path: resolve(output, `${view.name}-download.png`) });
    await page.locator('#pdfDownloadOpenFolderBtn').click();
    await page.locator('#pdfDownloadOverlay').waitFor({ state: 'detached' });
    assert.equal(await page.evaluate(() => window.reportCalls.filter(call => call.command === 'open_path_in_file_manager').at(-1).args.path), '/synthetic/downloads');
    assert.equal(await page.evaluate(() => window.reportCalls.filter(call => ['sync_now', 'get_supabase_client', 'sign_in'].includes(call.command)).length), 0);
    assert.deepEqual(externalRequests, [], 'The isolated report scenario must not contact external services.');
    assert.deepEqual(errors, []);
    results.push({ scenario: view.name, synthetic: true, pages: pages.length, bytes: exported.bytes.length,
      generationViaUi: true, privateChoiceExcluded: true, technicalNarrativeExcluded: true, downloadDialog: true,
      retryTested: view.name === 'wide-en', noExternalRequests: true });
    await context.close();
  }
  await writeFile(resolve(output, 'export-verification.json'), JSON.stringify({ passed: true, synthetic: true, results }, null, 2));
  console.log(JSON.stringify({ passed: true, synthetic: true, results }));
} finally {
  if (browser) await browser.close();
  if (server) await new Promise(resolve => server.close(resolve));
}
