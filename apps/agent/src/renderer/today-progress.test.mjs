import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { calendarEventElapsedProgress, formatCompactDuration } from './today-progress.mjs';

const markup = readFileSync(new URL('./index.html', import.meta.url), 'utf8');
const styles = readFileSync(new URL('./calendar-companion.css', import.meta.url), 'utf8');

test('calendar progress uses the event window, not tracked daily time', () => {
  const event = {
    startAt: '2026-09-30T18:30:00+02:00',
    endAt: '2026-09-30T19:30:00+02:00',
  };
  assert.deepEqual(calendarEventElapsedProgress(event, Date.parse('2026-09-30T16:45:00Z')), {
    elapsedSeconds: 900,
    totalSeconds: 3600,
    fraction: 0.25,
    percent: 25,
  });
  assert.equal(calendarEventElapsedProgress({ ...event, provider: 'microsoft' }, Date.parse('2026-09-30T16:45:00Z')).percent, 25);
  assert.equal(calendarEventElapsedProgress(event, Date.parse('2026-09-30T16:00:00Z')).percent, 0);
  assert.equal(calendarEventElapsedProgress(event, Date.parse('2026-09-30T18:00:00Z')).percent, 100);
});

test('invalid or zero-length calendar windows do not show a misleading bar', () => {
  assert.equal(calendarEventElapsedProgress(null), null);
  assert.equal(calendarEventElapsedProgress({ startAt: 'bad', endAt: '2026-09-30T19:30:00+02:00' }), null);
  assert.equal(calendarEventElapsedProgress({ startAt: '2026-09-30T19:30:00+02:00', endAt: '2026-09-30T19:30:00+02:00' }), null);
});

test('compact durations work for calendar-event progress', () => {
  assert.equal(formatCompactDuration(0), '0m');
  assert.equal(formatCompactDuration(900), '15m');
  assert.equal(formatCompactDuration(4500), '1h 15m');
});

test('Today only shows the calendar-event rail, while Daily goal keeps its text label', () => {
  const event = markup.indexOf('id="todayCalendarContext"');
  const eventProgress = markup.indexOf('id="todayCalendarProgressTrack"');
  const goalLabel = markup.indexOf('<span class="timer-meta-label">Daily goal</span>');
  assert.ok(event < eventProgress && eventProgress < goalLabel);
  assert.doesNotMatch(markup, /id="todayGoalProgressTrack"/);
  assert.match(markup, /Calendar time elapsed/);
  assert.match(markup, /aria-label="Calendar event elapsed time"/);
  assert.match(styles, /\.today-calendar-progress-track\s*>\s*span\s*\{[^}]*display:\s*block/);
});
