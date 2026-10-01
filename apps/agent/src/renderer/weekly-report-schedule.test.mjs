import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { isWeeklyReportDue } from './weekly-report-schedule.mjs';

test('early schedule checks preserve the promise contract used on window focus', () => {
  const source = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
  assert.match(source, /function checkScheduledReport\(\) \{\s*if \(!weeklyReportsReady\) return Promise\.resolve\(\);\s*if \(scheduledReportCheckPromise\) return scheduledReportCheckPromise;/);
});

test('runs on the selected local day at or after the selected time', () => {
  const schedule = { enabled: true, weekday: 5, time: '17:00' };
  assert.equal(isWeeklyReportDue(schedule, new Date(2026, 9, 2, 16, 59)), false);
  assert.equal(isWeeklyReportDue(schedule, new Date(2026, 9, 2, 17, 0)), true);
  assert.equal(isWeeklyReportDue(schedule, new Date(2026, 9, 2, 19, 30)), true);
  assert.equal(isWeeklyReportDue(schedule, new Date(2026, 9, 3, 19, 30)), false);
  assert.equal(isWeeklyReportDue({ ...schedule, enabled: false }, new Date(2026, 9, 2, 19, 30)), false);
});

test('does not run again in the same ISO week even after a schedule change', () => {
  const schedule = { enabled: true, weekday: 6, time: '17:00', lastGeneratedDate: '2026-10-02' };
  assert.equal(isWeeklyReportDue(schedule, new Date(2026, 9, 3, 17, 0)), false);
  assert.equal(isWeeklyReportDue({ ...schedule, weekday: 5 }, new Date(2026, 9, 9, 17, 0)), true);
});

test('week boundaries use local dates across a new year', () => {
  const schedule = { enabled: true, weekday: 5, time: '09:00', lastGeneratedDate: '2026-12-31' };
  assert.equal(isWeeklyReportDue(schedule, new Date(2027, 0, 1, 9, 0)), false);
  assert.equal(isWeeklyReportDue(schedule, new Date(2027, 0, 8, 9, 0)), true);
});
