import { t, message, getLocale } from './i18n.mjs';

const number = (value) => value == null || value === '' || !Number.isFinite(Number(value))
  ? null : Math.max(0, Number(value));
const display = (value) => new Intl.NumberFormat(getLocale(), { maximumFractionDigits: 1, minimumFractionDigits: 1 }).format(value);
const range = (data) => [data?.period_start, data?.period_end].filter(Boolean).join(' – ');

function measures(data = {}) {
  const focus = data.focus_semantics || data;
  const eligible = number(data.focus_eligible_seconds ?? focus.focus_eligible_seconds);
  const deep = number(data.deep_focus_seconds ?? focus.deep_focus_seconds);
  const detours = number(data.distraction_seconds ?? focus.distraction_seconds);
  const longest = number(focus.longest_focus_seconds);
  const total = number(data.total_seconds);
  const available = total > 0 && eligible > 0 && deep !== null && detours !== null;
  const share = eligible > 0 && deep !== null ? Math.min(100, deep / eligible * 100) : null;
  const detourRate = eligible > 0 && detours !== null ? detours / eligible * 60 : null;
  // A transparent pattern indicator: hours, calendar coverage and collaboration
  // never earn points. Both periods use the same denominators and weights.
  const score = available ? Math.round((0.7 * share / 100 + 0.3 * eligible / (eligible + detours)) * 100) / 10 : null;
  return { available, score, share, detourRate, longest: longest === null ? null : longest / 60 };
}

export function createReportProgress(local = {}, periodDays = 7) {
  const current = measures(local);
  const prior = local.prior_period;
  const previous = measures(prior);
  // Require the actual matching window, even for cached reports. Today compares
  // the same weekday; 30-day reviews compare the preceding non-overlapping month.
  const start = Date.parse(`${local.period_start}T12:00:00Z`);
  const end = Date.parse(`${local.period_end}T12:00:00Z`);
  const offset = (periodDays === 1 ? 7 : periodDays) * 86400000;
  const matches = Number.isFinite(start) && Number.isFinite(end)
    && range(prior) === range({ period_start: new Date(start - offset).toISOString().slice(0, 10), period_end: new Date(end - offset).toISOString().slice(0, 10) });
  const comparable = current.available && previous.available && matches;
  const delta = comparable ? Math.round((current.score - previous.score) * 10) / 10 : null;
  const direction = delta > 0 ? 'up' : delta < 0 ? 'down' : 'steady';
  const rows = [
    { key: 'focus', label: t('Sustained focus'), current: current.share, previous: previous.share, unit: '%', higher: true, threshold: 1,
      better: t('More time in sustained focus'), worse: t('Recover your sustained focus'),
      advice: t('Protect a 30-minute block for one task and keep optional checks for the break.') },
    { key: 'detours', label: t('Distraction minutes per focus hour'), current: current.detourRate, previous: previous.detourRate, unit: 'min/h', higher: false, threshold: 0.5,
      better: t('Fewer distractions per focus hour'), worse: t('Reduce returns to distractions'),
      advice: t('Mute optional notifications and batch app checks into a planned break.') },
    { key: 'longest', label: t('Longest focus block'), current: current.longest, previous: previous.longest, unit: 'min', higher: true, threshold: 5,
      better: t('Longer uninterrupted blocks'), worse: t('Rebuild one uninterrupted block'),
      advice: t('Choose one task before starting and finish the block before switching context.') },
  ].filter((row) => row.current !== null && current.available).map((row) => {
    const change = comparable && row.previous !== null ? (row.current - row.previous) * (row.higher ? 1 : -1) : null;
    return { ...row, change, currentLabel: `${display(row.current)} ${row.unit}`, previousLabel: comparable && row.previous !== null ? `${display(row.previous)} ${row.unit}` : t('No baseline'),
      tone: change >= row.threshold ? 'up' : change <= -row.threshold ? 'down' : 'steady' };
  });
  const item = (row, improved) => ({ title: improved ? row.better : row.worse,
    evidence: comparable && row.previous !== null ? `${row.previousLabel} → ${row.currentLabel}` : row.currentLabel,
    body: improved ? '' : row.advice });
  const improvements = rows.filter((row) => row.tone === 'up').map((row) => item(row, true)).slice(0, 2);
  const opportunities = rows.filter((row) => row.tone === 'down').map((row) => item(row, false)).slice(0, 2);
  if (!opportunities.length && current.available) {
    const target = rows.find((row) => row.key === 'focus' && row.current < 60)
      || rows.find((row) => row.key === 'detours' && row.current > 5);
    opportunities.push(target ? {
      title: target.key === 'focus' ? t('Keep building your sustained focus') : t('Keep reducing distraction time'),
      evidence: message`Current: ${target.currentLabel}`,
      body: target.advice,
    } : { title: t('Keep the conditions that work'), evidence: '', body: t('Repeat your strongest focus block and check whether the pattern holds next week.') });
  }
  return {
    score: current.score, scoreLabel: current.score === null ? '—' : display(current.score),
    previousScoreLabel: comparable ? display(previous.score) : '—', comparable, direction, delta,
    deltaLabel: delta === null ? '' : `${delta > 0 ? '+' : delta < 0 ? '−' : ''}${display(Math.abs(delta))}`,
    title: periodDays === 1 ? t('Your focus today') : periodDays > 7 ? t('Your focus this period') : t('Your focus this week'),
    comparisonLabel: periodDays === 1 ? t('vs. the same day last week') : periodDays > 7 ? t('vs. the previous 30 days') : t('vs. last week'),
    previousPeriod: matches ? range(prior) : '',
    verdict: current.score === null ? t('Not enough focus data') : current.score >= 8 ? t('Strong focus rhythm') : current.score >= 6 ? t('A solid foundation') : t('Room to build your focus'),
    status: comparable ? (direction === 'up' ? t('Your focus improved') : direction === 'down' ? t('Your focus needs attention') : t('Your focus is stable')) : t('Building your baseline'),
    note: !current.available ? t('Record a focus block to receive a grade. Time spent in meetings alone does not define your focus.')
        : !comparable ? t('There is not enough comparable focus data from the previous period. Your grade uses this period only.')
        : t('Both grades use the same method. Changes describe recorded patterns.'),
    method: t('Grade out of 10: 70% sustained-focus share of eligible work + 30% time free of detected distractions. More hours do not earn extra points. Meetings and collaboration are outside this focus grade; it does not measure work quality.'),
    improvements, opportunities, rows,
    improvementEmpty: !comparable ? t('Your first comparison will appear when both periods have focus data.') : t('No clear improvement in these measures this period. Stable patterns are also useful.'),
    opportunityEmpty: t('Record a focus block first; there is not enough evidence for a specific change.'),
    coverage: message`${local.active_days || 0} days with recorded activity`,
  };
}
