import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, getLocale, getLanguage, initializeLocalization, categoryLabel } from './i18n.mjs';
import { createReportProgress } from './report-progress.mjs';
const DAY_MS = 24 * 60 * 60 * 1000;

export function cleanReportText(value) {
  if (value == null) return '';
  const cleaned = String(value);
  const segments = cleaned
    .split(/(?<=[.!?])\s+|\n+/)
    .map((part) => part.trim())
    .filter(Boolean);
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

function reportTimestamp(value) {
  const text = cleanReportText(value);
  if (!/^\d{4}-\d{2}-\d{2}T/.test(text)) return text;
  const time = Date.parse(text);
  return Number.isFinite(time) ? new Intl.DateTimeFormat(getLocale(), { dateStyle: 'medium', timeStyle: 'short' }).format(time) : text;
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
    label: new Intl.DateTimeFormat(getLocale(), { weekday: 'short', timeZone: 'UTC' })
      .format(new Date(`${date}T12:00:00Z`)),
    seconds: totals.get(date) || 0,
    hours: ((totals.get(date) || 0) / 3600).toFixed(1),
    displayDuration: durationLabel(totals.get(date) || 0),
    percentOfPeak: Math.round(((totals.get(date) || 0) / max) * 100),
  }));
}

function reportTone(status) {
  const normalized = status.toLowerCase();
  if (/risk|attention|fragment|riesgo|atenci[oó]n/.test(normalized)) return 'attention';
  if (/sustained blocks observed|bloques sostenidos observados|se observaron bloques sostenidos/.test(normalized)) return 'positive';
  return 'neutral';
}

function reportLessons(report, { categories, days, totalSeconds, focusSeconds, focusSessions, activeDays }) {
  const supplied = asItems(report.lessons_learned)
    .map((lesson) => ({
      title: cleanReportText(lesson?.title),
      body: cleanReportText(lesson?.body),
    }))
    .filter((lesson) => lesson.title && lesson.body);
  if (!totalSeconds) return [];
  if (supplied.length) return supplied;

  // Older or incomplete local-AI payloads still need evidence-backed lessons
  // in both the on-screen review and the PDF exported from that same model.
  const lessons = [];
  const top = categories[0];
  if (top) {
    lessons.push({
      title: tr('The work mix had a clear centre'),
      body: formatMessage`${top.displayDuration} (${top.percent}% of tracked time) was categorised as ${top.label}. Compare that mix with your intended priorities; time distribution alone is not an outcome measure.`,
    });
  }
  if (focusSeconds > 0) {
    const blocks = focusSessions > 0 ? formatMessage` across ${focusSessions} sustained blocks` : '';
    lessons.push({
      title: tr('Sustained work was visible'),
      body: formatMessage`${(focusSeconds / 3600).toFixed(1)}h of sustained focus was recorded${blocks}. Use the recorded block boundaries to identify conditions worth repeating, without treating duration as a productivity score.`,
    });
  }
  if (days.length > 0 && activeDays > 0 && activeDays < days.length) {
    lessons.push({
      title: tr('Coverage limits the conclusion'),
      body: formatMessage`Activity was recorded on ${activeDays} of ${days.length} days. Days without recorded activity do not prove that no work happened.`,
    });
  }
  if (!lessons.length) {
    lessons.push({
      title: tr('Recorded time is a starting point'),
      body: formatMessage`${(totalSeconds / 3600).toFixed(1)}h was recorded, but category and focus signals are too limited for a specific workflow conclusion. Add task context or compare another period before changing plans.`,
    });
  }
  return lessons;
}

function durationLabel(seconds) {
  if (!seconds) return '0 min';
  if (seconds < 60) return '<1 min';
  const hours = seconds >= 3600;
  const value = hours ? seconds / 3600 : seconds / 60;
  const decimals = hours ? 1 : seconds < 600 && seconds % 60 ? 1 : 0;
  return `${new Intl.NumberFormat(getLocale(), { minimumFractionDigits: hours ? 1 : 0, maximumFractionDigits: decimals }).format(value)} ${hours ? 'h' : 'min'}`;
}

// The main review follows measured data. Generated narrative remains available
// in the screen's detail disclosure, rather than competing with the charts.
function reportEditorial(model) {
  const { categories, days, progress, activeDays, periodDays } = model;
  const appContexts = model.distractions.apps.filter(app => app.kind !== 'other').slice(0, 3);
  const appMessage = model.distractions.state === 'ready' && !appContexts.length
    ? tr('The recorded app changes need context before drawing a conclusion.') : model.distractions.message;
  const top = categories[0];
  const peak = [...days].sort((a, b) => b.seconds - a.seconds)[0];
  const chartCategories = categories.length > 6 ? [
    ...categories.slice(0, 5),
    { label: tr('Other categories'), seconds: categories.slice(5).reduce((sum, row) => sum + row.seconds, 0),
      percent: model.totalSeconds ? Math.min(100, Math.round(categories.slice(5).reduce((sum, row) => sum + row.seconds, 0) / model.totalSeconds * 100)) : 0 },
  ] : categories;
  const observations = [];
  if (top) observations.push({ title: top.label,
    evidence: `${top.displayDuration} · ${top.percent}%`,
    body: tr('The largest part of your recorded time. Check whether it matches your priorities.') });
  if (model.focusSeconds > 0) observations.push({ title: tr('Sustained focus'),
    evidence: tr('{duration} in {count} blocks', { duration: model.focusDuration, count: model.focusSessions }),
    body: tr('Blocks of at least 25 minutes in eligible work. Meetings and collaboration have their own value.') });
  const actions = [];
  if (model.empty) return { chartCategories: [], appContexts: [], appMessage, observations: [], actions: [],
    lead: tr('No local activity was recorded in this period.'),
    timeline: tr('Days without recorded activity do not prove that no work happened.'),
    coverage: tr('This covers recorded activity, not your whole working day.') };
  if (activeDays < periodDays) actions.push({ title: tr('Check the coverage first'),
    evidence: tr('{active} of {total} days recorded', { active: activeDays, total: periodDays }),
    body: tr('Compare the gaps with your calendar before changing your routine. A blank day may simply have no tracking data.') });
  const opportunity = progress.opportunities[0];
  if (opportunity) actions.push({ title: opportunity.title, evidence: opportunity.evidence, body: opportunity.body });
  if (top) actions.push({ title: tr('Align your time with your priorities'),
    evidence: `${top.label} · ${top.displayDuration} · ${top.percent}%`,
    body: tr('Choose the main task for your next session. Then check whether your time distribution reflects what you intended to work on.') });
  if (actions.length < 3 && appContexts.length) actions.push({ title: tr('Give app changes context'),
    evidence: tr('App visits are observations, not proof of distraction.'),
    body: tr('Keep tools needed for work available. Move only optional checks to a break, then see whether your focus feels easier.') });
  if (!actions.length) actions.push({ title: tr('Build a useful baseline'), evidence: model.totalDuration,
    body: tr('Record another normal work session and compare it with your plans before making a specific change.') });
  return {
    chartCategories: chartCategories.map(row => ({ ...row, displayDuration: durationLabel(row.seconds) })),
    appContexts, appMessage, observations, actions: actions.slice(0, 3),
    lead: tr('{duration} recorded across {active} days.', { duration: model.totalDuration, active: activeDays }),
    timeline: peak?.seconds ? tr('Most recorded time: {day}, {duration}.', { day: peak.label, duration: peak.displayDuration })
      : tr('No dated activity available.'),
    coverage: tr('{active} of {total} days recorded', { active: activeDays, total: periodDays }),
  };
}

function distractionAdvice(appName, kind, workInterleavedRevisits) {
  if (kind === 'music') {
    return workInterleavedRevisits
      ? formatMessage`Choose a playlist in ${appName} before the next work block, leave playback running, and batch track changes into one break. Check whether you return less often next session.`
      : formatMessage`Choose a playlist in ${appName} before the work block and leave playback in the background; save track changes for a break.`;
  }
  if (kind === 'video') {
    return formatMessage`Queue or save what you want to watch in ${appName} for a planned break, then close it during the work block.`;
  }
  if (kind === 'communication') {
    return formatMessage`If ${appName} is not needed for live collaboration, mute it for the next focus block and check it at a chosen interval; keep urgent contacts available.`;
  }
  return formatMessage`Decide whether ${appName} belongs to the current task. If not, close it for one focus block and move optional checks to a planned break.`;
}

function reportDistractionApps(local) {
  const analysis = local.distraction_app_analysis;
  if (!analysis || analysis.unavailable || !Array.isArray(analysis.detours)) {
    return {
      state: 'unavailable', apps: [],
      message: analysis?.unavailable
        ? tr('Foreground destination analysis could not be completed. Try generating the report again.')
        : tr('This report predates app and site context analysis. Generate a new report to see it.'),
      caveat: '',
    };
  }
  const maxSeconds = Math.max(1, ...analysis.detours.map((row) => asSeconds(row.seconds)));
  const apps = analysis.detours
    .map((row) => {
      const appName = String(row?.label ?? '').replace(/\s+/g, ' ').trim().slice(0, 72);
      const kind = String(row?.kind ?? 'social');
      const seconds = asSeconds(row?.seconds);
      const visits = Math.max(0, Number(row?.visits) || 0);
      const days = Math.max(0, Number(row?.days) || 0);
      const workInterleavedRevisits = Math.max(0, Number(row?.work_interleaved_revisits) || 0);
      const daily = asItems(row?.daily_visits)
        .map((day) => ({
          date: isoDay(day?.date), seconds: asSeconds(day?.seconds),
          visits: Math.max(0, Number(day?.visits) || 0),
          workInterleavedRevisits: Math.max(0, Number(day?.work_interleaved_revisits) || 0),
          shortestRevisitMinutes: Math.max(0, Number(day?.shortest_revisit_minutes) || 0),
          observedAt: asItems(day?.observed_at)
            .map((time) => String(time).trim())
            .filter((time) => /^([01]\d|2[0-3]):[0-5]\d$/.test(time)),
        }))
        .filter((day) => day.date && day.visits > 0)
        .sort((left, right) => left.date.localeCompare(right.date));
      const repeatedDay = daily.find((day) => day.date === isoDay(local.period_end) && day.visits >= 2)
        || [...daily].reverse().find((day) => day.visits >= 2);
      const dayLabel = repeatedDay?.date === isoDay(local.period_end) ? tr('Today') : repeatedDay?.date;
      const observedTimes = repeatedDay?.observedAt.slice(0, 4) || [];
      const timeList = observedTimes.length < 2 ? ''
        : observedTimes.length === 2 ? observedTimes.join(tr(' and '))
          : formatMessage`${observedTimes.slice(0, -1).join(', ')} and ${observedTimes.at(-1)}`;
      const repeated = repeatedDay
        ? formatMessage`${dayLabel}: ${repeatedDay.visits} foreground sightings${timeList ? formatMessage` around ${timeList}` : ''} (${durationLabel(repeatedDay.seconds)} sampled).`
          + (repeatedDay.workInterleavedRevisits
            ? formatMessage` Work screens appeared between sightings; ${appName} reappeared ${repeatedDay.workInterleavedRevisits === 1 ? tr('once') : formatMessage`${repeatedDay.workInterleavedRevisits} times`}${repeatedDay.shortestRevisitMinutes ? formatMessage`, with the shortest interval ${repeatedDay.shortestRevisitMinutes} min` : ''}.`
            : '')
        : '';
      const periodSummary = days === 1 && repeatedDay ? ''
        : formatMessage`${visits} foreground ${visits === 1 ? tr('sighting') : tr('sightings')} across ${days} ${days === 1 ? tr('day') : tr('days')}; ${durationLabel(seconds)} on screen. `;
      const observed = `${periodSummary}${repeated || (workInterleavedRevisits ? formatMessage`${workInterleavedRevisits} revisits had work screens in between.` : '')}`.trim();
      return {
        appName, kind, seconds, visits, days, workInterleavedRevisits, daily,
        duration: durationLabel(seconds),
        percentOfTop: Math.max(2, Math.round(seconds / maxSeconds * 100)),
        observed,
        advice: distractionAdvice(appName, kind, workInterleavedRevisits),
      };
    })
    .filter((row) => row.appName && row.seconds > 0)
    .slice(0, 5);
  return {
    state: apps.length ? 'ready' : 'none',
    apps,
    message: tr('No recurring work-to-app return or sustained casual-browsing destination was observed in this period.'),
    caveat: tr('Based on sampled foreground screens; background playback is not included.'),
  };
}

export function createStatusReportViewModel(payload, { userName = tr('Knowledge worker'), todayDate = '' } = {}) {
  const report = payload?.localized_report?.[getLanguage()] || payload?.report || {};
  const local = payload?.local_data || {};
  const meta = report.report_meta || {};
  const text = (value) => cleanReportText(value);
  const list = (value) => asItems(value).map(text).filter(Boolean);
  const totalSeconds = asSeconds(local.total_seconds);
  const focusSeconds = asSeconds(local.deep_focus_seconds);
  const focusSessions = Math.max(0, Number(local.deep_focus_sessions) || 0);
  const days = reportDays(local);
  const activeDays = Math.max(0, Number(local.active_days) || days.filter((day) => day.seconds > 0).length);
  const periodStart = isoDay(local.period_start);
  const periodEnd = isoDay(local.period_end);
  const period = text(meta.period_label)
    || (periodStart && periodStart === periodEnd ? periodStart : [periodStart, periodEnd].filter(Boolean).join(' – '))
    || todayDate;
  const status = text(report.overall_health) || tr('No assessment available');
  const categories = asItems(local.category_breakdown)
    .map((row) => ({ label: categoryLabel(text(row.category) === 'CodeReview' ? 'Code Review' : text(row.category)) || tr('Unlabelled'), seconds: asSeconds(row.total_seconds) }))
    .filter((row) => row.seconds > 0)
    .sort((a, b) => b.seconds - a.seconds)
    .map((row) => ({
      ...row,
      hours: (row.seconds / 3600).toFixed(1),
      displayDuration: durationLabel(row.seconds),
      percent: totalSeconds ? Math.min(100, Math.round(row.seconds / totalSeconds * 100)) : 0,
    }));
  const distractions = reportDistractionApps(local);
  const periodDays = days.length || Math.max(0, Number(local.period_days) || 0);

  const model = {
    title: periodDays === 1 ? tr('Daily work review') : periodDays > 7 ? tr('Work review') : tr('Weekly work review'),
    evidenceTitle: periodDays === 1 ? tr('The day in view') : periodDays > 7 ? tr('The period in view') : tr('The week in view'),
    period,
    periodStart,
    periodEnd,
    userName: text(userName) || tr('Knowledge worker'),
    generatedAt: reportTimestamp(payload?.generated_at) || todayDate,
    summary: text(report.executive_overview || report.work_summary)
      || (totalSeconds ? tr('Activity was recorded in this period.') : tr('No local activity was recorded in this period.')),
    status,
    statusTone: reportTone(status),
    healthNotes: text(report.health_notes),
    focusTarget: text(report.focus_target || meta.focus_target),
    timelineCaption: text(report.timeline_caption || report.work_summary),
    totalHours: new Intl.NumberFormat(getLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(totalSeconds / 3600),
    focusHours: new Intl.NumberFormat(getLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(focusSeconds / 3600),
    totalSeconds,
    focusSeconds,
    totalDuration: durationLabel(totalSeconds),
    focusDuration: durationLabel(focusSeconds),
    focusSessions,
    activeDays,
    periodDays,
    progress: createReportProgress(local, periodDays),
    empty: totalSeconds === 0,
    days,
    categories,
    distractions,
    actions: totalSeconds > 0 ? list(report.recommendations) : [],
    breakdown: asItems(report.health_breakdown).map((row) => ({
      element: text(row.element) || tr('Work area'),
      status: text(row.status) || tr('Observed'),
      notes: text(row.notes),
      owner: text(row.owner_team),
    })),
    knownIssues: list(report.known_issues),
    potentialRisks: list(report.potential_risks),
    observedWork: list(report.observed_work),
    highlights: list(report.work_progress),
    lessons: reportLessons(report, { categories, days, totalSeconds, focusSeconds, focusSessions, activeDays }),
    lessonEmptyMessage: tr('No activity was recorded, so there is not enough evidence to draw a lesson for this period.'),
    aiPowered: Boolean(payload?.ai_powered),
  };
  model.editorial = reportEditorial(model);
  model.sourceActions = model.actions;
  model.actions = model.editorial.actions.map(action => action.body);
  return model;
}

function escapeHtml(value) {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function reportList(items, emptyLabel) {
  return items.length
    ? html`<ul class="sr-plain-list">${items.map((item) => html`<li>${escapeHtml(item)}</li>`).join('')}</ul>`
    : html`<p class="sr-muted">${escapeHtml(emptyLabel)}</p>`;
}

function directionIcon(direction) {
  const path = direction === 'up' ? 'M5 17 17 5M5 5h12v12' : direction === 'down' ? 'M5 5l12 12M5 17h12V5' : 'M4 11h14';
  return `<svg width="16" height="16" viewBox="0 0 22 22" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${path}"/></svg>`;
}

function renderReportGrade(progress) {
  if (!progress) return '';
  return html`<section class="sr-grade" aria-labelledby="srGradeTitle">
    <h3 id="srGradeTitle">${escapeHtml(progress.title)}</h3>
    <div class="sr-grade-main">
      <div class="sr-grade-number" aria-label="${escapeHtml(formatMessage`Focus grade: ${progress.scoreLabel} out of 10`)}"><strong>${progress.scoreLabel}</strong><span>/10</span></div>
      <div class="sr-grade-reading"><strong>${escapeHtml(progress.verdict)}</strong>
        ${progress.comparable ? html`<span class="sr-grade-delta sr-trend-${progress.direction}">${directionIcon(progress.direction)} ${escapeHtml(progress.deltaLabel)} <span>points</span></span>` : ''}
        <span>${escapeHtml(progress.comparable ? progress.comparisonLabel : progress.status)}</span>
      </div>
    </div>
    ${progress.score !== null ? html`<div class="sr-grade-scale" aria-hidden="true">${Array.from({length:10},(_,i)=>`<span class="${progress.score >= i + 1 ? 'is-filled' : ''}"></span>`).join('')}</div>` : ''}
    <p class="sr-grade-note">${escapeHtml(progress.note)}</p>
  </section>`;
}

function renderReportProgress(progress) {
  if (!progress) return '';
  const insightList = (items, empty) => items.length ? html`<ul class="sr-progress-insights">${items.map((item) => html`<li><strong>${escapeHtml(item.title)}</strong>${item.evidence ? html`<span class="sr-progress-evidence">${escapeHtml(item.evidence)}</span>` : ''}${item.body ? html`<p>${escapeHtml(item.body)}</p>` : ''}</li>`).join('')}</ul>` : html`<p class="sr-progress-empty">${escapeHtml(empty)}</p>`;
  const rows = progress.rows.map((row) => {
    const max = row.key === 'focus' ? 100 : Math.max(1, row.current, progress.comparable ? row.previous || 0 : 0);
    const width = (value) => Math.min(100, Math.max(0, value / max * 100));
    return html`<div class="sr-progress-measure"><div class="sr-progress-measure-title"><strong>${escapeHtml(row.label)}</strong>${progress.comparable ? html`<span class="sr-trend-${row.tone}">${directionIcon(row.tone)}</span>` : ''}</div>
      ${progress.comparable && row.previous !== null ? html`<div class="sr-comparison-bar sr-comparison-previous" aria-label="${escapeHtml(formatMessage`Previous: ${row.previousLabel}`)}"><span style="width:${width(row.previous)}%"></span><b>${escapeHtml(row.previousLabel)}</b></div>` : ''}
      <div class="sr-comparison-bar sr-comparison-current" aria-label="${escapeHtml(formatMessage`Current: ${row.currentLabel}`)}"><span style="width:${width(row.current)}%"></span><b>${escapeHtml(row.currentLabel)}</b></div>
    </div>`;
  }).join('');
  return html`<section class="sr-progress" aria-label="Your progress">
    <div class="sr-progress-columns">
      <div class="sr-progress-improved"><h3>${directionIcon('up')} What improved</h3>${insightList(progress.improvements, progress.improvementEmpty)}</div>
      <div class="sr-progress-next"><h3>${directionIcon('steady')} Where to improve</h3>${insightList(progress.opportunities, progress.opportunityEmpty)}</div>
    </div>
    ${rows ? html`<div class="sr-progress-comparison"><div class="sr-progress-comparison-heading"><h3>${escapeHtml(progress.comparable ? tr('The comparison behind your grade') : tr('Your current focus measures'))}</h3>
      ${progress.comparable ? html`<p>Previous <strong>${escapeHtml(progress.previousScoreLabel)}/10</strong> <span aria-hidden="true">→</span> Current <strong>${escapeHtml(progress.scoreLabel)}/10</strong></p><p class="sr-progress-period">${escapeHtml(progress.previousPeriod)} ${escapeHtml(progress.comparisonLabel)}</p>` : ''}</div>
      <div class="sr-comparison-legend" aria-hidden="true">${progress.comparable ? html`<span class="sr-legend-previous">Previous</span>` : ''}<span class="sr-legend-current">Current</span></div><div class="sr-progress-measures">${rows}</div></div>` : ''}
    <details class="sr-grade-method"><summary>How is this grade calculated?</summary><p>${escapeHtml(progress.method)}</p></details>
  </section>`;
}

export function renderStatusReportHtml(model) {
  const days = model.days.map((day) => html`
    <div class="sr-day" title="${escapeHtml(day.date)}: ${escapeHtml(day.displayDuration)}" aria-label="${escapeHtml(day.date)}: ${escapeHtml(day.displayDuration)}">
      <div class="sr-day-value">${day.displayDuration}</div>
      <div class="sr-day-track"><span style="height:${day.seconds ? Math.max(7, day.percentOfPeak) : 0}%"></span></div>
      <div class="sr-day-label">${escapeHtml(day.label)}<small>${escapeHtml(day.date.slice(8))}</small></div>
    </div>`).join('');
  const categories = model.editorial.chartCategories.length
    ? model.editorial.chartCategories.map((category) => html`
      <div class="sr-category-row">
        <span class="sr-category-name" title="${escapeHtml(category.label)}">${escapeHtml(category.label)}</span>
        <div class="sr-category-track" aria-label="${escapeHtml(formatMessage`${category.percent}% of tracked time`)}"><span style="width:${Math.max(2, category.percent)}%"></span></div>
        <strong>${category.displayDuration}</strong>
      </div>`).join('')
    : markup('<p class="sr-muted">No category time recorded.</p>');
  const actions = model.actions.length
    ? html`<ol class="sr-action-list">${model.editorial.actions.map((action, index) => html`
        <li><div><strong>${escapeHtml(action.title)}</strong><span class="sr-progress-evidence">${escapeHtml(action.evidence)}</span><p>${escapeHtml(action.body)}</p><button type="button" class="button button-ghost sr-choice-btn" data-review-suggestion="${index}">Try this change</button></div></li>`).join('')}</ol>`
    : markup('<p class="sr-muted">No specific next move is supported by this period yet. Keep tracking to build a baseline.</p>');
  const breakdown = model.breakdown.length
    ? html`<div class="sr-signal-list">${model.breakdown.map((row) => html`
        <div class="sr-signal-row"><div class="sr-signal-top"><strong>${escapeHtml(row.element)}</strong><span>${escapeHtml(row.status)}</span></div>
          ${row.notes ? html`<p>${escapeHtml(row.notes)}</p>` : ''}
          ${row.owner && row.owner.toLowerCase() !== 'self' ? html`<small>${escapeHtml(formatMessage`Owner: ${row.owner}`)}</small>` : ''}
        </div>`).join('')}</div>`
    : markup('<p class="sr-muted">No work-area detail was generated.</p>');
  const lessons = model.lessons.length
    ? html`<div class="sr-lessons">${model.lessons.map((lesson) => html`<div><strong>${escapeHtml(lesson.title)}</strong><p>${escapeHtml(lesson.body)}</p></div>`).join('')}</div>`
    : html`<p class="sr-muted">${escapeHtml(model.lessonEmptyMessage)}</p>`;
  const distractions = model.editorial.appContexts.length
    ? html`<div class="sr-distraction-list">${model.editorial.appContexts.map((app) => html`
      <div class="sr-distraction-row">
        <div class="sr-distraction-measure">
          <div class="sr-distraction-top"><strong title="${escapeHtml(app.appName)}">${escapeHtml(app.appName)}</strong><span>${escapeHtml(app.duration)}</span></div>
          <div class="sr-distraction-track" role="img" aria-label="${escapeHtml(formatMessage`${app.appName}: ${app.duration} in sampled foreground visits`)}"><span style="width:${app.percentOfTop}%"></span></div>
        </div>
        <div class="sr-distraction-detail">
          <p class="sr-distraction-observed">${escapeHtml(tr('{visits} foreground visits across {days} days', { visits: app.visits, days: app.days }))}</p>
          <details class="sr-app-evidence"><summary>See recorded context</summary><p>${escapeHtml(app.observed)}</p></details>
        </div>
      </div>`).join('')}</div>`
    : html`<p class="sr-muted">${escapeHtml(model.editorial.appMessage)}</p>`;

  return html`
    <article class="status-report sr-review">
      <header class="sr-review-hero">
        <div class="sr-review-heading"><div><h2>${escapeHtml(model.title)}</h2><p>${escapeHtml(model.period)}</p></div>
          </div>
        <p class="sr-review-meta">${escapeHtml(model.userName)} <span aria-hidden="true">·</span> ${escapeHtml(formatMessage`Generated ${model.generatedAt}`)}</p>
        ${renderReportGrade(model.progress)}
        <p class="sr-origin">${escapeHtml(model.aiPowered ? tr('Local AI report') : tr('Rule-based report'))} · ${escapeHtml(formatMessage`${model.activeDays} of ${model.periodDays} days with activity`)}. ${escapeHtml(tr('This covers recorded activity, not your whole working day.'))}</p>
      </header>

      <section class="sr-overview" aria-label="Review summary">
        <p>${escapeHtml(model.editorial.lead)}</p>
        <div class="sr-stat-line">
          <div><strong>${model.totalDuration}</strong><span>Tracked time</span></div>
          <div><strong>${model.focusDuration}</strong><span>${escapeHtml(formatMessage`Sustained focus · ${model.focusSessions} blocks`)}</span></div>
          <div><strong>${model.activeDays}<span>/${model.periodDays}</span></strong><span>Days with activity</span></div>
        </div>
      </section>

      <section class="sr-section" aria-labelledby="srEvidenceTitle">
        <div class="sr-section-heading"><h3 id="srEvidenceTitle">${escapeHtml(model.evidenceTitle)}</h3><p>Recorded time, not a productivity score</p></div>
        <div class="sr-evidence-grid">
          <figure class="sr-figure"><figcaption>Activity by day</figcaption>
            ${days ? html`<div class="sr-day-chart">${days}</div>` : markup('<p class="sr-muted">No dated activity available.</p>')}
          </figure>
          <figure class="sr-figure"><figcaption>Time by category</figcaption>${categories}</figure>
        </div>
        <p class="sr-evidence-note">${escapeHtml(model.editorial.timeline)}</p>
      </section>

      ${renderReportProgress(model.progress)}

      <section class="sr-section sr-next" aria-labelledby="srNextTitle">
        <div class="sr-section-heading"><h3 id="srNextTitle">What to do next</h3><p>Choose one change. Review it after your next normal work session.</p></div>
        ${actions}
      </section>

      <div id="workReviewSlot"></div>

      <section class="sr-section sr-distractions" aria-labelledby="srDistractionsTitle">
        <div class="sr-section-heading"><h3 id="srDistractionsTitle">App changes, with context</h3><p>App visits are observations, not proof of distraction.</p></div>
        ${distractions}
        <p class="sr-distraction-caveat">Keep tools needed for work available. Only you can confirm whether a visit was optional.</p>
        ${model.distractions.caveat ? html`<p class="sr-distraction-caveat" id="srDistractionCaveat">${escapeHtml(model.distractions.caveat)}</p>` : ''}
      </section>

      <details class="sr-report-details"><summary>Read the full local analysis</summary>
      <section class="sr-section sr-context" aria-labelledby="srContextTitle">
        <div class="sr-section-heading"><h3 id="srContextTitle">How to read the signal</h3></div>
        <p>${escapeHtml(model.summary)}</p>
        ${model.healthNotes ? html`<p>${escapeHtml(model.healthNotes)}</p>` : markup('<p class="sr-muted">No additional interpretation was generated.</p>')}
        ${model.focusTarget ? html`<p class="sr-focus-target"><strong>Focus target</strong> ${escapeHtml(model.focusTarget)}</p>` : ''}
        ${reportList(model.sourceActions, '')}
        ${model.distractions.apps.length ? html`<h4>App changes, with context</h4>${reportList(model.distractions.apps.map(app => `${app.appName}: ${app.observed}`), '')}` : ''}
      </section>

      <section class="sr-section" aria-labelledby="srAreasTitle">
        <div class="sr-section-heading"><h3 id="srAreasTitle">Work-area detail</h3><p>Specific observations behind the review</p></div>
        ${breakdown}
      </section>

      <section class="sr-section sr-detail-grid" aria-label="Observed work and watchpoints">
        <div><h3>Work observed</h3>${reportList(model.observedWork, tr('No labelled work was observed.'))}
          ${model.highlights.length ? html`<h4>Highlights</h4>${reportList(model.highlights, '')}` : ''}</div>
        <div><h3>Watchpoints</h3><h4>Known issues</h4>${reportList(model.knownIssues, tr('None flagged.'))}
          <h4>Potential risks</h4>${reportList(model.potentialRisks, tr('None flagged.'))}</div>
      </section>

      <section class="sr-section" aria-labelledby="srLessonsTitle"><div class="sr-section-heading"><h3 id="srLessonsTitle">What this period taught us</h3></div>${lessons}</section>
      </details>

      <footer class="sr-review-footer"><p>${escapeHtml(formatMessage`Based on activity stored on this device. ${model.aiPowered ? tr('Narrative assisted by local AI.') : tr('Structured, rule-based narrative.')} Interpret alongside your own context.`)}</p>
        <button type="button" class="sr-download-btn" id="downloadReportPdfBtn" aria-label="Download weekly work review as PDF">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 3v12m0 0 4-4m-4 4-4-4M4 17v3h16v-3" stroke-linecap="round" stroke-linejoin="round"/></svg>
          Download PDF
        </button></footer>
    </article>`;
}
