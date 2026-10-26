import test from 'node:test';
import assert from 'node:assert/strict';
import { createReportProgress } from './report-progress.mjs';
import { setLanguagePreference } from './i18n.mjs';
setLanguagePreference('en', { persist: false });

const metrics = (overrides = {}) => ({ total_seconds: 36000, focus_eligible_seconds: 18000, deep_focus_seconds: 10800, distraction_seconds: 1800, longest_focus_seconds: 2700, ...overrides });
const fixture = () => ({ ...metrics(), period_start: '2026-10-12', period_end: '2026-10-18', active_days: 5,
  prior_period: { ...metrics({ deep_focus_seconds: 7200, distraction_seconds: 3600, longest_focus_seconds: 1800 }), period_start: '2026-10-05', period_end: '2026-10-11' } });

test('grade and improvements compare rates using the same method in adjacent weeks', () => {
  const result = createReportProgress(fixture());
  assert.equal(result.score, 6.9);
  assert.equal(result.previousScoreLabel, '5.3');
  assert.equal(result.delta, 1.6);
  assert.equal(result.comparable, true);
  assert.equal(result.improvements.length, 2);
  assert.equal(result.rows.find(row => row.key === 'detours').tone, 'up');
  assert.equal(result.rows.find(row => row.key === 'longest').tone, 'up');
  assert.equal(result.opportunities[0].title, 'Keep reducing distraction time');
  assert.equal(result.opportunities[0].evidence, 'Current: 6.0 min/h');
});

test('longer tracked time and more days never raise the grade by themselves', () => {
  const local = fixture();
  const score = createReportProgress(local).score;
  local.total_seconds *= 4; local.active_days = 7;
  assert.equal(createReportProgress(local).score, score);
  local.focus_eligible_seconds *= 2; local.deep_focus_seconds *= 2; local.distraction_seconds *= 2;
  assert.equal(createReportProgress(local).score, score);
});

test('worsening signals give a negative delta and concrete next steps without invented improvements', () => {
  const local = fixture();
  Object.assign(local, metrics({ deep_focus_seconds: 3600, distraction_seconds: 5400, longest_focus_seconds: 900 }));
  const result = createReportProgress(local);
  assert.ok(result.delta < 0);
  assert.equal(result.direction, 'down');
  assert.equal(result.improvements.length, 0);
  assert.equal(result.opportunities.length, 2);
  assert.ok(result.opportunities.every(item => item.body.length > 20));
  assert.ok(result.opportunities.every(item => item.evidence.includes('→')));
});

test('remaining focus goals use the current level without presenting an observed gain as a decline', () => {
  const local = fixture();
  local.deep_focus_seconds = 7200; local.distraction_seconds = 0;
  local.prior_period.deep_focus_seconds = 3600; local.prior_period.distraction_seconds = 0;
  const result = createReportProgress(local);
  assert.equal(result.opportunities[0].title, 'Keep building your sustained focus');
  assert.equal(result.opportunities[0].evidence, 'Current: 40.0 %');
  assert.ok(result.improvements.some(item => item.evidence.includes('20.0 % → 40.0 %')));
});

test('stable rates do not claim an improvement', () => {
  const local = fixture(); Object.assign(local.prior_period, metrics());
  const result = createReportProgress(local);
  assert.equal(result.delta, 0);
  assert.equal(result.improvements.length, 0);
  assert.ok(result.rows.every(row => row.tone === 'steady'));
});

test('empty, context-only and incomplete legacy payloads do not receive fabricated grades', () => {
  for (const local of [{}, metrics({ total_seconds: 0 }), metrics({ focus_eligible_seconds: 0 }), metrics({ distraction_seconds: undefined }), metrics({ focus_eligible_seconds: NaN })]) {
    const result = createReportProgress(local);
    assert.equal(result.score, null);
    assert.equal(result.delta, null);
    assert.equal(result.improvements.length, 0);
    assert.equal(result.opportunities.length, 0);
  }
  const local = fixture(); delete local.prior_period.distraction_seconds;
  assert.equal(createReportProgress(local).comparable, false);
  assert.equal(createReportProgress(local).score, 6.9);
});

test('daily review uses the same weekday last week; monthly comparison never overlaps', () => {
  const local = fixture();
  local.period_start = local.period_end = '2026-10-18';
  local.prior_period.period_start = local.prior_period.period_end = '2026-10-11';
  assert.equal(createReportProgress(local, 1).comparable, true);
  local.prior_period.period_start = local.prior_period.period_end = '2026-10-17';
  assert.equal(createReportProgress(local, 1).comparable, false);
  local.period_start = '2026-09-19';
  local.prior_period.period_start = '2026-08-20'; local.prior_period.period_end = '2026-09-18';
  assert.equal(createReportProgress(local, 30).comparable, true);
  local.prior_period.period_start = '2026-09-12'; local.prior_period.period_end = '2026-10-11';
  assert.equal(createReportProgress(local, 30).comparable, false);
});

test('Spanish grade, numbers, explanations and next steps are localized', () => {
  setLanguagePreference('es', { persist: false });
  try {
    const result = createReportProgress(fixture());
    assert.equal(result.scoreLabel, '6,9');
    assert.equal(result.title, 'Tu foco esta semana');
    assert.match(result.improvements[0].title, /Más tiempo/);
    assert.match(result.method, /Más horas no dan/);
  } finally { setLanguagePreference('en', { persist: false }); }
});
