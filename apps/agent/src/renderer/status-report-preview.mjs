// Local visual fixture. All names and activity below are illustrative.
import { readFileSync, writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { jsPDF } from 'jspdf';

import { createStatusReportViewModel, renderStatusReportHtml } from './status-report.mjs';
import { renderStatusReportPdf } from './status-report-pdf.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const appHtml = readFileSync(join(here, 'index.html'), 'utf8');
const style = appHtml.match(/<style>([\s\S]*?)<\/style>/)?.[1];
if (!style) throw new Error('Could not read the app stylesheet');

const syntheticPayload = {
  generated_at: '2026-09-27 12:00',
  ai_powered: true,
  local_data: {
    period_start: '2026-09-21', period_end: '2026-09-27', period_days: 7,
    total_seconds: 107_280, deep_focus_seconds: 32_040, deep_focus_sessions: 6, active_days: 5,
    daily_totals: [
      { date: '2026-09-21', total_seconds: 18_000 },
      { date: '2026-09-22', total_seconds: 23_040 },
      { date: '2026-09-23', total_seconds: 15_120 },
      { date: '2026-09-24', total_seconds: 28_800 },
      { date: '2026-09-25', total_seconds: 22_320 },
    ],
    category_breakdown: [
      { category: 'Building', total_seconds: 50_400 },
      { category: 'Planning', total_seconds: 30_240 },
      { category: 'Meetings', total_seconds: 18_000 },
      { category: 'Documentation', total_seconds: 8_640 },
    ],
  },
  report: {
    executive_overview: 'You recorded 29.8 hours across five active days. Building was the largest observed work area, while six sustained focus blocks totalled 8.9 hours. This review describes recorded activity; it does not measure output quality.',
    overall_health: 'Sustained blocks observed',
    health_notes: 'Sustained work appeared on several days, with the highest tracked total on Thursday. The shorter Wednesday does not by itself indicate a problem; compare it with your calendar before adjusting your schedule.',
    focus_target: 'Protect uninterrupted build time when it matches planned priorities.',
    timeline_caption: 'Thursday held the highest recorded total. Building and planning accounted for most of the week.',
    recommendations: [
      'Reserve one build block at the start of the next workday, then compare its recorded duration with this week’s sustained blocks.',
      'Review Wednesday’s calendar and captured activities before drawing a conclusion from its lower tracked time.',
      'Add a short task label to planning work when you want its continuity reflected in the next review.',
    ],
    health_breakdown: [
      { element: 'Building', status: 'Sustained-work eligible', owner_team: 'Self', notes: 'Fourteen hours were recorded in the largest work area.' },
      { element: 'Planning', status: 'Observed', owner_team: 'Self', notes: '8.4 hours were recorded across the review period.' },
      { element: 'Meetings', status: 'Context work', owner_team: 'Self', notes: 'Coordination work remains visible and should not be treated as a distraction.' },
    ],
    known_issues: ['No known tracking issue was flagged in the recorded activity.'],
    potential_risks: ['The review cannot infer work quality or explain gaps without additional context.'],
    observed_work: ['Building work led the recorded categories.', 'Planning appeared throughout the workweek.'],
    work_progress: ['A total of 29.8 hours was recorded across five days.'],
    lessons_learned: [
      { title: 'Use the calendar as context', body: 'A shorter tracked day can reflect planned coordination or time away from the desk.' },
      { title: 'Keep labels lightweight', body: 'Simple task labels make future comparisons more useful without requiring a separate project system.' },
    ],
  },
};

const payloadPath = process.argv[2];
const payload = payloadPath ? JSON.parse(readFileSync(payloadPath, 'utf8')) : syntheticPayload;
const model = createStatusReportViewModel(payload, {
  userName: payloadPath ? 'Local user' : 'Illustrative user',
});
const outputDir = process.argv[3] || mkdtempSync(join(tmpdir(), 'flowsight-report-preview-'));
const htmlPath = join(outputDir, 'review.html');
const lightHtmlPath = join(outputDir, 'review-light.html');
const pdfPath = join(outputDir, 'review.pdf');
function previewHtml(extraCss = '') {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${style}\nhtml,body{height:auto;overflow:visible}body{display:block;margin:0;padding:32px;background:#eeeaf4}.sr-review{max-width:900px;margin:auto;box-shadow:0 16px 50px #2420331a}@media(max-width:600px){body{padding:0}}${extraCss}</style></head><body>${renderStatusReportHtml(model)}</body></html>`;
}
writeFileSync(htmlPath, previewHtml());
writeFileSync(lightHtmlPath, previewHtml(`:root{color-scheme:light;--background:225 24% 97%;--foreground:225 24% 11%;--card:0 0% 100%;--primary:263 84% 58%;--primary-foreground:0 0% 100%;--muted:222 22% 94%;--muted-foreground:220 11% 43%;--border:220 18% 87%}`));
writeFileSync(pdfPath, Buffer.from(renderStatusReportPdf(new jsPDF(), model).output('arraybuffer')));
console.log(htmlPath);
console.log(lightHtmlPath);
console.log(pdfPath);
