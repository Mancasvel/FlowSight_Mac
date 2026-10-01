import test from 'node:test';
import assert from 'node:assert/strict';
import { jsPDF } from 'jspdf';
import { setLanguagePreference } from './i18n.mjs';
setLanguagePreference('en', { persist: false });

import { createStatusReportViewModel } from './status-report.mjs';
import { renderStatusReportPdf } from './status-report-pdf.mjs';

function syntheticReport() {
  return {
    generated_at: '2026-09-28 12:00',
    ai_powered: false,
    local_data: {
      period_start: '2026-09-22',
      period_end: '2026-09-28',
      period_days: 7,
      total_seconds: 36_000,
      focus_seconds: 14_400,
      deep_focus_seconds: 14_400,
      active_days: 2,
      daily_totals: [
        { date: '2026-09-22', total_seconds: 14_400 },
        { date: '2026-09-25', total_seconds: 21_600 },
      ],
      category_breakdown: [
        { category: 'Coding', total_seconds: 21_600 },
        { category: 'Planning', total_seconds: 14_400 },
      ],
    },
    report: {
      executive_overview: 'Ten hours were recorded across two active days.',
      overall_health: 'Review the pattern',
      health_notes: 'Category time describes what was tracked, not work quality.',
      recommendations: ['Compare the quieter day with your calendar.'],
      health_breakdown: [{ element: 'Coding', status: 'Observed', owner_team: 'Self', notes: 'Six hours recorded.' }],
      known_issues: ['Coverage is sparse.'],
      potential_risks: ['This period may under-represent work.'],
      observed_work: ['Coding and planning were observed.'],
      work_progress: ['Ten hours were recorded.'],
      lessons_learned: [{ title: 'Add context', body: 'Task labels make future reviews more useful.' }],
    },
  };
}

test('the PDF model displays deep-focus time using measured sustained intervals', () => {
  const model = createStatusReportViewModel(syntheticReport(), { userName: 'Sample user' });
  assert.equal(model.focusHours, '4.0');
  assert.equal(model.days.length, 7);
  assert.equal(model.days[1].seconds, 0);
  assert.equal(model.categories[0].percent, 60);
  assert.equal(model.activeDays, 2);
  const pdf = renderStatusReportPdf(new jsPDF({ compress: false }), model).output();
  assert.match(pdf, /SUSTAINED FOCUS/);
  assert.doesNotMatch(pdf, /FOCUS-CATEGORY TIME/);
  assert.match(pdf, /Rule-based narrative/);
});

test('a concise work review fits in two PDF pages', () => {
  const model = createStatusReportViewModel(syntheticReport());
  const doc = renderStatusReportPdf(new jsPDF(), model);
  assert.ok(doc.internal.getNumberOfPages() <= 2);
});

test('long report text paginates without writing below the footer', () => {
  const payload = syntheticReport();
  const long = 'Review each observed block before changing the schedule. '.repeat(23);
  payload.report.recommendations = Array.from({ length: 5 }, (_, index) => `Action ${index + 1}: ${long} END_ACTION_${index + 1}`);
  payload.report.health_breakdown = Array.from({ length: 6 }, (_, index) => ({
    element: `Work area ${index + 1}`,
    status: 'Observed',
    notes: `${long} END_AREA_${index + 1}`,
  }));
  const model = createStatusReportViewModel(payload);
  const doc = new jsPDF({ compress: false });
  const originalText = doc.text.bind(doc);
  const drawn = [];
  doc.text = (value, x, y, ...rest) => {
    drawn.push({ value: String(value), y });
    assert.ok(y <= 291, `text baseline ${y} exceeded the page footer`);
    return originalText(value, x, y, ...rest);
  };
  renderStatusReportPdf(doc, model);
  assert.ok(doc.internal.getNumberOfPages() >= 3);
  assert.ok(drawn.some(({ value }) => value.includes('END_ACTION_5')));
  assert.ok(drawn.some(({ value }) => value.includes('END_AREA_6')));
});
