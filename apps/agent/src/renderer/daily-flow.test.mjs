import test from 'node:test';
import assert from 'node:assert/strict';
import { dailyFlowView } from './daily-flow.mjs';

const now = new Date(2026, 9, 3, 12).getTime();
const progress = (completed_dates, total_seconds = 0, date = '2026-10-03') => ({ completed_dates, total_seconds, date });

test('daily wins require persisted evidence; passive counter progress never creates a win', () => {
  const before = dailyFlowView(progress([], 899), now);
  assert.equal(before.todayDone, false);
  assert.equal(before.remainingMinutes, 1);
  const unsaved = dailyFlowView(progress([], 900), now);
  assert.equal(unsaved.totalWins, 0);
  const saved = dailyFlowView(progress(['2026-10-03'], 900), now);
  assert.equal(saved.todayDone, true);
  assert.equal(saved.totalWins, 1);
  assert.equal(saved.percent, 100);
});

test('duplicates, future dates and malformed dates cannot inflate wins', () => {
  const view = dailyFlowView(progress(['2026-10-01', '2026-10-01', '2026-10-04', '2026-09-99', null]), now);
  assert.equal(view.totalWins, 1);
  assert.equal(view.weekWins, 1);
  assert.equal(view.milestone.days, 3);
});

test('streak crosses week and month boundaries and stays until today ends', () => {
  const view = dailyFlowView(progress(['2026-09-28', '2026-09-29', '2026-09-30', '2026-10-01', '2026-10-02']), now);
  assert.equal(view.streak, 5);
  assert.equal(view.todayDone, false);
  assert.equal(view.weekWins, 5);
  const monday = new Date(2026, 8, 28, 12).getTime();
  assert.equal(dailyFlowView(progress(['2026-09-26', '2026-09-27'], 0, '2026-09-28'), monday).streak, 2);
});

test('rest days reset streak while keeping cumulative milestones', () => {
  const view = dailyFlowView(progress(['2026-09-25', '2026-09-26', '2026-09-27']), now);
  assert.equal(view.streak, 0);
  assert.equal(view.totalWins, 3);
  assert.equal(view.milestone.days, 7);
});

test('stale previous-day progress resets countdown at local midnight', () => {
  const view = dailyFlowView(progress(['2026-10-02'], 3600, '2026-10-02'), now);
  assert.equal(view.percent, 0);
  assert.equal(view.remainingMinutes, 15);
  assert.equal(view.streak, 1);
});

test('local week and streak handle the Madrid DST change', () => {
  const dst = new Date(2026, 9, 26, 12).getTime();
  const view = dailyFlowView(progress(['2026-10-24', '2026-10-25'], 0, '2026-10-26'), dst);
  assert.equal(view.streak, 2);
  assert.equal(view.week[0].key, '2026-10-26');
  assert.equal(view.week[6].key, '2026-11-01');
  assert.equal(view.weekWins, 0);
});

test('zero and invalid progress stays a useful first-run state', () => {
  const view = dailyFlowView(null, now);
  assert.equal(view.totalWins, 0);
  assert.equal(view.milestone.days, 1);
  assert.equal(dailyFlowView(progress([], Infinity), now).percent, 0);
});

test('native lifetime totals and streak survive retention with only current-week dates', () => {
  const view = dailyFlowView({ date: '2026-10-03', total_seconds: 900, completed_dates: ['2026-10-03'], total_wins: 70, streak: 70 }, now);
  assert.equal(view.totalWins, 70);
  assert.equal(view.streak, 70);
  assert.equal(view.milestone, null);
  assert.equal(view.weekWins, 1);
});

test('activity streak preserves thirteen days independently of five daily wins', () => {
  const today = new Date(2026, 9, 7, 12).getTime();
  const view = dailyFlowView({ date: '2026-10-07', total_seconds: 900,
    completed_dates: ['2026-10-05', '2026-10-06', '2026-10-07'], total_wins: 5, streak: 13 }, today);
  assert.equal(view.streak, 13);
  assert.equal(view.totalWins, 5);
  assert.equal(view.weekWins, 3);
  assert.equal(view.milestone.days, 7);
});
