const DAY_MS = 24 * 60 * 60 * 1000;

export function cleanReportText(value) {
  if (value == null) return '';
  const cleaned = String(value).replace(/[\u4E00-\u9FFF\u3400-\u4DBF\u3040-\u30FF\uAC00-\uD7AF]/g, ' ');
  const segments = cleaned
    .split(/(?<=[.!?])\s+|\n+/)
    .map((part) => part.trim())
    .filter(Boolean)
    .filter((part) => {
      const englishLetters = part.replace(/[^A-Za-z]/g, '').length;
      const latinLetters = part.replace(/[^A-Za-z\u00C0-\u024F]/g, '').length;
      return latinLetters === 0 || englishLetters / latinLetters >= 0.55;
    });
  return (segments.length ? segments.join(' ') : cleaned).replace(/\s+/g, ' ').trim();
}

function asItems(value) {
  return Array.isArray(value) ? value : [];
}

function asSeconds(value) {
  const seconds = Number(value);
  return Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
}

function isoDay(value) {
  const match = String(value ?? '').match(/^\d{4}-\d{2}-\d{2}$/);
  return match ? match[0] : '';
}

function reportDays(local) {
  const totals = new Map(asItems(local.daily_totals)
    .filter((day) => isoDay(day.date))
    .map((day) => [day.date, asSeconds(day.total_seconds)]));
  const start = isoDay(local.period_start);
  const end = isoDay(local.period_end);
  const startTime = start ? Date.parse(`${start}T12:00:00Z`) : NaN;
  const endTime = end ? Date.parse(`${end}T12:00:00Z`) : NaN;
  const dates = [];
  if (Number.isFinite(startTime) && Number.isFinite(endTime)
      && endTime >= startTime && endTime - startTime < 31 * DAY_MS) {
    for (let time = startTime; time <= endTime; time += DAY_MS) {
      dates.push(new Date(time).toISOString().slice(0, 10));
    }
  } else {
    dates.push(...[...totals.keys()].sort());
  }
  const max = Math.max(1, ...dates.map((date) => totals.get(date) || 0));
  return dates.map((date) => ({
    date,
    label: new Intl.DateTimeFormat('en', { weekday: 'short', timeZone: 'UTC' })
      .format(new Date(`${date}T12:00:00Z`)),
    seconds: totals.get(date) || 0,
    hours: ((totals.get(date) || 0) / 3600).toFixed(1),
    percentOfPeak: Math.round(((totals.get(date) || 0) / max) * 100),
  }));
}

function reportTone(status) {
  const normalized = status.toLowerCase();
  if (/risk|attention|fragment/.test(normalized)) return 'attention';
  if (/sustained blocks observed/.test(normalized)) return 'positive';
  return 'neutral';
}

export function createStatusReportViewModel(payload, { userName = 'Knowledge worker', todayDate = '' } = {}) {
  const report = payload?.report || {};
  const local = payload?.local_data || {};
  const meta = report.report_meta || {};
  const text = (value) => cleanReportText(value);
  const list = (value) => asItems(value).map(text).filter(Boolean);
  const totalSeconds = asSeconds(local.total_seconds);
  const focusSeconds = asSeconds(local.focus_seconds);
  const days = reportDays(local);
  const period = text(meta.period_label)
    || [isoDay(local.period_start), isoDay(local.period_end)].filter(Boolean).join(' – ')
    || todayDate;
  const status = text(report.overall_health) || 'No assessment available';
  const categories = asItems(local.category_breakdown)
    .map((row) => ({ label: text(row.category) || 'Unlabelled', seconds: asSeconds(row.total_seconds) }))
    .filter((row) => row.seconds > 0)
    .sort((a, b) => b.seconds - a.seconds)
    .map((row) => ({
      ...row,
      hours: (row.seconds / 3600).toFixed(1),
      percent: totalSeconds ? Math.min(100, Math.round(row.seconds / totalSeconds * 100)) : 0,
    }));

  return {
    title: 'Weekly work review',
    period,
    userName: text(userName) || 'Knowledge worker',
    generatedAt: text(payload?.generated_at) || todayDate,
    summary: text(report.executive_overview || report.work_summary)
      || (totalSeconds ? 'Activity was recorded in this period.' : 'No local activity was recorded in this period.'),
    status,
    statusTone: reportTone(status),
    healthNotes: text(report.health_notes),
    focusTarget: text(report.focus_target || meta.focus_target),
    timelineCaption: text(report.timeline_caption || report.work_summary),
    totalHours: (totalSeconds / 3600).toFixed(1),
    focusHours: (focusSeconds / 3600).toFixed(1),
        activeDays: Math.max(0, Number(local.active_days) || days.filter((day) => day.seconds > 0).length),
    periodDays: days.length || Math.max(0, Number(local.period_days) || 0),
    empty: totalSeconds === 0,
    days,
    categories,
    actions: list(report.recommendations),
    breakdown: asItems(report.health_breakdown).map((row) => ({
      element: text(row.element) || 'Work area',
      status: text(row.status) || 'Observed',
      notes: text(row.notes),
      owner: text(row.owner_team),
    })),
    knownIssues: list(report.known_issues),
    potentialRisks: list(report.potential_risks),
    observedWork: list(report.observed_work),
    highlights: list(report.work_progress),
    lessons: asItems(report.lessons_learned).map((lesson) => ({
      title: text(lesson.title) || 'Learning',
      body: text(lesson.body),
    })),
    aiPowered: Boolean(payload?.ai_powered),
  };
}
