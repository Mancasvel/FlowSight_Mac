import test from 'node:test';
import assert from 'node:assert/strict';

import {
  bucketFocusEntriesByHour,
  buildTaskBreakdown,
  focusChartSlots,
  formatChartHour,
  taskColorForCategory,
  taskSharePercent,
} from './insights-charts.mjs';

process.env.TZ = 'Europe/Madrid';

test('task colors follow the activity type, not the ranking', () => {
  assert.equal(taskColorForCategory('Coding'), 'var(--task-color-coding)');
  assert.equal(taskColorForCategory('Code Review'), 'var(--task-color-review)');
  assert.equal(taskColorForCategory('Design'), 'var(--task-color-design)');
  assert.equal(taskColorForCategory('Analysis'), 'var(--task-color-analysis)');
  assert.equal(taskColorForCategory('Browsing'), 'var(--task-color-browsing)');
  assert.equal(taskColorForCategory('General'), 'var(--task-color-general)');
  assert.notEqual(taskColorForCategory('Analysis'), taskColorForCategory('General'));
  assert.notEqual(taskColorForCategory('General'), taskColorForCategory('Browsing'));
  assert.notEqual(taskColorForCategory('Coding'), taskColorForCategory('Design'));

  const data = {
    ticket_breakdown: [
      { ticket: 'FS-1', total_seconds: 150 },
      { ticket: 'FS-2', total_seconds: 50 },
    ],
    entries: [
      { ticket: 'FS-1', category: 'Coding', duration_seconds: 120 },
      { ticket: 'FS-1', category: 'Debugging', duration_seconds: 30 },
      { ticket: 'FS-2', category: 'Design', duration_seconds: 50 },
      { ticket: null, category: 'Planning', duration_seconds: 80 },
    ],
  };
  const items = buildTaskBreakdown(data);
  assert.equal(items.find(item => item.label === 'FS-1').category, 'Coding');
  assert.equal(items.find(item => item.label === 'FS-2').category, 'Design');
  assert.equal(items.find(item => item.label === 'Planning').category, 'Planning');
  assert.equal(taskColorForCategory(items.find(item => item.label === 'FS-1').category), 'var(--task-color-coding)');
  assert.equal(taskSharePercent(48, 100), 48);
  assert.equal(taskSharePercent(150, 100), 100);
  assert.equal(taskSharePercent(10, 0), 0);
});

test('focus durations span the real local hours, including outside office hours', () => {
  const byHour = bucketFocusEntriesByHour([
    { time: '2026-09-28T07:20:00Z', duration_seconds: 3000 },
    { time: '2026-09-28T21:30:00Z', duration_seconds: 3600 },
  ], '2026-09-28');
  assert.equal(byHour[8], 1800);
  assert.equal(byHour[9], 1200);
  assert.equal(byHour[22], 1800);
  assert.equal(byHour[23], 1800);
  const slots = focusChartSlots(byHour);
  assert.equal(slots[0].hour, 7);
  assert.equal(slots.at(-1).hour, 23);
  assert.equal(formatChartHour(slots.at(-1).hour), '11pm');
});

test('focus time crossing midnight is clipped to the displayed local date', () => {
  const entries = [{ time: '2026-09-28T22:10:00Z', duration_seconds: 1800 }];
  const today = bucketFocusEntriesByHour(entries, '2026-09-29');
  assert.equal(today[0], 600);
  assert.equal(today.reduce((sum, seconds) => sum + seconds, 0), 600);
  const yesterday = bucketFocusEntriesByHour(entries, '2026-09-28');
  assert.equal(yesterday[23], 1200);
});

test('repeated local hour at DST change keeps both intervals', () => {
  const byHour = bucketFocusEntriesByHour([
    { time: '2026-10-25T01:30:00Z', duration_seconds: 3600 },
  ], '2026-10-25');
  assert.equal(byHour[2], 3600);
  assert.equal(byHour.reduce((sum, seconds) => sum + seconds, 0), 3600);
});
