import { t as tr } from './i18n.mjs';

const LEFT = 18, RIGHT = 192, WIDTH = RIGHT - LEFT;
const INK = [24, 42, 52], MUTED = [77, 98, 106], TEAL = [8, 111, 107];
const RULE = [214, 226, 227], TRACK = [234, 241, 241], PREVIOUS = [128, 151, 157];

function style(doc, size = 10, weight = 'normal', color = INK) {
  doc.setFont('helvetica', weight);
  doc.setFontSize(size);
  doc.setTextColor(...color);
}

function printable(value) {
  return String(value ?? '').replaceAll('→', ' -> ').replaceAll('−', '-').replaceAll('—', '-');
}

// The PDF is a bounded editorial review. Full generated copy and sampled
// timestamps remain available in the on-screen detail disclosure.
function paragraph(doc, value, x, y, width, { size = 10, leading = 5, color = INK, weight = 'normal', maxLines = 4 } = {}) {
  style(doc, size, weight, color);
  let lines = doc.splitTextToSize(printable(value), width);
  if (lines.length > maxLines) {
    lines = lines.slice(0, maxLines);
    let last = lines.at(-1);
    while (doc.getTextWidth(`${last}...`) > width) last = last.slice(0, -1);
    lines[lines.length - 1] = `${last.trimEnd()}...`;
  }
  for (const line of lines) {
    doc.text(line, x, y);
    y += leading;
  }
  return y;
}

function line(doc, y) {
  doc.setDrawColor(...RULE);
  doc.line(LEFT, y, RIGHT, y);
}

function header(doc, model, title, first = false) {
  if (!first) doc.addPage();
  style(doc, 10, 'bold', TEAL);
  doc.text('FlowSight', LEFT, 18);
  style(doc, 8, 'normal', MUTED);
  doc.text(first ? printable(model.generatedAt) : printable(model.period), RIGHT, 18, { align: 'right' });
  line(doc, 24);
  return paragraph(doc, title, LEFT, 38, WIDTH, { size: 22, weight: 'bold', leading: 9, maxLines: 2 }) + 2;
}

function sectionTitle(doc, title, y, note = '') {
  style(doc, 14, 'bold');
  doc.text(title, LEFT, y);
  if (note) paragraph(doc, note, LEFT, y + 7, WIDTH, { size: 9, color: MUTED, maxLines: 2 });
}

function metrics(doc, model, y) {
  const entries = [[model.totalDuration, tr('Tracked time')],
    [model.focusDuration, tr('Sustained focus')], [`${model.activeDays}/${model.periodDays}`, tr('Days with activity')]];
  entries.forEach(([value, label], index) => {
    const x = LEFT + index * WIDTH / 3;
    style(doc, 21, 'bold');
    doc.text(value, x, y);
    paragraph(doc, label, x, y + 7, WIDTH / 3 - 6, { size: 9, color: MUTED, maxLines: 2 });
  });
}

function grade(doc, progress, y) {
  style(doc, 9, 'bold', MUTED);
  doc.text(progress.title, LEFT, y);
  if (progress.score === null) return paragraph(doc, progress.note, LEFT, y + 9, WIDTH, { size: 10, color: MUTED, maxLines: 3 });
  style(doc, 36, 'bold', TEAL);
  doc.text(`${progress.scoreLabel}/10`, LEFT, y + 20);
  paragraph(doc, progress.verdict, LEFT + 62, y + 8, WIDTH - 62, { size: 12, weight: 'bold', maxLines: 1 });
  if (progress.comparable) {
    paragraph(doc, `${printable(progress.deltaLabel)} ${tr('points')} ${progress.comparisonLabel}`, LEFT + 62, y + 16, WIDTH - 62,
      { size: 10, color: TEAL, maxLines: 2 });
    paragraph(doc, `${tr('Previous')} ${progress.previousScoreLabel}/10`, LEFT + 62, y + 27, WIDTH - 62,
      { size: 9, color: MUTED, maxLines: 1 });
  } else paragraph(doc, progress.status, LEFT + 62, y + 17, WIDTH - 62, { size: 10, color: MUTED, maxLines: 2 });
}

function daysChart(doc, days, x, y, width, height = 48) {
  style(doc, 10, 'bold');
  doc.text(tr('Activity by day'), x, y);
  if (!days.length) return paragraph(doc, tr('No dated activity available.'), x, y + 12, width, { size: 9, color: MUTED });
  const max = Math.max(1, ...days.map(day => day.seconds));
  const gap = days.length === 1 ? 0 : 2;
  const barWidth = Math.min(17, (width - gap * (days.length - 1)) / days.length);
  const chartTop = y + 16, bottom = chartTop + height;
  days.forEach((day, index) => {
    const at = x + index * (barWidth + gap);
    style(doc, 7.4, 'normal', MUTED);
    doc.text(day.displayDuration, at + barWidth / 2, chartTop - 4, { align: 'center' });
    doc.setFillColor(...TRACK);
    doc.roundedRect(at, chartTop, barWidth, height, 1, 1, 'F');
    if (day.seconds) {
      const fillHeight = Math.max(0.8, day.seconds / max * height);
      doc.setFillColor(...TEAL);
      doc.roundedRect(at, bottom - fillHeight, barWidth, fillHeight, 0.5, 0.5, 'F');
    }
    style(doc, 8, 'bold');
    doc.text(day.label, at + barWidth / 2, bottom + 6, { align: 'center' });
    style(doc, 7.5, 'normal', MUTED);
    doc.text(day.date.slice(8), at + barWidth / 2, bottom + 11, { align: 'center' });
  });
}

function categoriesChart(doc, categories, x, y, width, rowHeight = 14) {
  style(doc, 10, 'bold');
  doc.text(tr('Time by category'), x, y);
  if (!categories.length) return paragraph(doc, tr('No category time recorded.'), x, y + 12, width, { size: 9, color: MUTED });
  const max = Math.max(1, ...categories.map(row => row.seconds));
  categories.forEach((row, index) => {
    const rowY = y + 13 + index * rowHeight;
    paragraph(doc, row.label, x, rowY, width - 25, { size: 9, maxLines: 1 });
    style(doc, 8.5, 'bold');
    doc.text(row.displayDuration, x + width, rowY, { align: 'right' });
    doc.setFillColor(...TRACK);
    doc.roundedRect(x, rowY + 3, width, 3, 1, 1, 'F');
    doc.setFillColor(...(index === 5 ? PREVIOUS : TEAL));
    doc.roundedRect(x, rowY + 3, Math.max(1, row.seconds / max * width), 3, 0.5, 0.5, 'F');
  });
}

function charts(doc, model, y) {
  if (model.days.length <= 7) {
    daysChart(doc, model.days, LEFT, y, 82);
    categoriesChart(doc, model.editorial.chartCategories, LEFT + 94, y, WIDTH - 94);
  } else {
    // Longer periods retain every date, in weekly rows with readable labels.
    style(doc, 10, 'bold');
    doc.text(tr('Activity by day'), LEFT, y);
    const peak = Math.max(1, ...model.days.map(day => day.seconds));
    model.days.forEach((day, index) => {
      const x = LEFT + index % 7 * 11.5, at = y + 12 + Math.floor(index / 7) * 17;
      style(doc, 7.5, 'normal', MUTED);
      doc.text(day.date.slice(8), x, at);
      doc.setFillColor(...TRACK);
      doc.rect(x + 4, at - 3, 7, 9, 'F');
      doc.setFillColor(...TEAL);
      if (day.seconds) doc.rect(x + 4, at + 6 - day.seconds / peak * 9, 7, day.seconds / peak * 9, 'F');
      style(doc, 6.8, 'normal', MUTED);
      doc.text(day.displayDuration, x, at + 10);
    });
    categoriesChart(doc, model.editorial.chartCategories, LEFT + 94, y, WIDTH - 94);
  }
}

function comparison(doc, progress, y) {
  if (!progress.rows.length) return paragraph(doc, progress.note, LEFT, y, WIDTH, { color: MUTED, maxLines: 3 });
  paragraph(doc, progress.comparable ? `${progress.previousPeriod} · ${progress.comparisonLabel}` : progress.note,
    LEFT, y, WIDTH, { size: 9, color: MUTED, maxLines: 2 });
  y += 16;
  progress.rows.forEach(row => {
    style(doc, 11, 'bold');
    doc.text(row.label, LEFT, y);
    const max = Math.max(row.key === 'focus' ? 100 : 1, row.current, row.previous || 0);
    const barX = LEFT + 29, barW = WIDTH - 62;
    const measure = (label, value, valueLabel, at, color) => {
      style(doc, 8.5, 'normal', MUTED);
      doc.text(label, LEFT, at + 4);
      doc.setFillColor(...TRACK);
      doc.roundedRect(barX, at, barW, 5, 1, 1, 'F');
      if (value > 0) {
        doc.setFillColor(...color);
        doc.roundedRect(barX, at, Math.max(1, value / max * barW), 5, 1, 1, 'F');
      }
      style(doc, 9, 'bold', color);
      doc.text(valueLabel, RIGHT, at + 4, { align: 'right' });
    };
    if (progress.comparable && row.previous !== null) measure(tr('Previous'), row.previous, row.previousLabel, y + 6, PREVIOUS);
    measure(tr('Current'), row.current, row.currentLabel, y + (progress.comparable ? 15 : 6), TEAL);
    y += progress.comparable ? 32 : 24;
  });
  return y;
}

function appContext(doc, model, y) {
  sectionTitle(doc, tr('App changes, with context'), y);
  y = paragraph(doc, tr('App visits are observations, not proof of distraction.'), LEFT, y + 8, WIDTH,
    { size: 9, color: MUTED, maxLines: 1 }) + 5;
  const apps = model.editorial.appContexts;
  if (!apps.length) return paragraph(doc, model.editorial.appMessage, LEFT, y, WIDTH, { size: 9, color: MUTED, maxLines: 2 });
  const max = Math.max(1, ...apps.map(app => app.seconds || 0));
  apps.forEach(app => {
    const name = /^[\u0020-\u00FF]+$/.test(app.appName) ? app.appName
      : tr('App name unavailable in this PDF font - see on-screen report');
    paragraph(doc, name, LEFT, y, 59, { size: 9, maxLines: 1 });
    doc.setFillColor(...TRACK);
    doc.roundedRect(LEFT + 63, y - 3, WIDTH - 95, 4, 1, 1, 'F');
    doc.setFillColor(...PREVIOUS);
    doc.roundedRect(LEFT + 63, y - 3, Math.max(1, (app.seconds || 0) / max * (WIDTH - 95)), 4, 0.5, 0.5, 'F');
    style(doc, 9, 'bold', MUTED);
    doc.text(app.duration, RIGHT, y, { align: 'right' });
    y += 11;
  });
  return paragraph(doc, tr('Keep tools needed for work available. Only you can confirm whether a visit was optional.'),
    LEFT, y + 1, WIDTH, { size: 8.5, color: MUTED, maxLines: 2 });
}

function footers(doc) {
  const count = doc.internal.getNumberOfPages();
  for (let page = 1; page <= count; page++) {
    doc.setPage(page);
    line(doc, 284);
    style(doc, 8, 'normal', MUTED);
    doc.text(tr('Local data · Focus patterns, with context'), LEFT, 291);
    doc.text(`${page} / ${count}`, RIGHT, 291, { align: 'right' });
  }
}

export function renderStatusReportPdf(doc, model) {
  let y = header(doc, model, model.title, true);
  paragraph(doc, model.period, LEFT, y, WIDTH, { size: 11, color: MUTED, maxLines: 1 });
  paragraph(doc, model.userName, LEFT, y + 8, WIDTH, { size: 9, color: MUTED, maxLines: 1 });
  if (model.empty) {
    sectionTitle(doc, tr('Not enough focus data'), 80);
    paragraph(doc, model.editorial.lead, LEFT, 96, WIDTH, { size: 12, maxLines: 2 });
    paragraph(doc, model.editorial.timeline, LEFT, 114, WIDTH, { color: MUTED, maxLines: 2 });
    paragraph(doc, tr('Record another normal work session and compare it with your plans before making a specific change.'),
      LEFT, 140, WIDTH, { maxLines: 3 });
    paragraph(doc, model.aiPowered ? tr('Local AI report') : tr('Rule-based report'), LEFT, 180, WIDTH,
      { size: 9, color: MUTED, maxLines: 1 });
    footers(doc);
    return doc;
  }
  grade(doc, model.progress, 70);
  paragraph(doc, `${model.aiPowered ? tr('Local AI report') : tr('Rule-based report')} · ${tr('{p0} of {p1} days with activity', { p0: model.activeDays, p1: model.periodDays })}`,
    LEFT, 106, WIDTH, { size: 8, color: MUTED, maxLines: 1 });
  metrics(doc, model, 120);
  line(doc, 139);
  sectionTitle(doc, model.evidenceTitle, 152, tr('Recorded time, not a productivity score'));
  charts(doc, model, 175);
  paragraph(doc, model.editorial.timeline, LEFT, 270, WIDTH, { size: 9, color: MUTED, maxLines: 2 });

  y = header(doc, model, tr('Your focus, in perspective'));
  sectionTitle(doc, model.progress.comparable ? tr('The comparison behind your grade') : tr('Your current focus measures'), y + 6);
  y = comparison(doc, model.progress, y + 20);
  y = Math.max(166, y + 5);
  sectionTitle(doc, tr('What improved'), y);
  y += 10;
  if (!model.progress.improvements.length) y = paragraph(doc, model.progress.improvementEmpty, LEFT, y, WIDTH,
    { size: 10, color: MUTED, maxLines: 2 }) + 5;
  else model.progress.improvements.slice(0, 2).forEach(item => {
    paragraph(doc, item.title, LEFT, y, WIDTH - 68, { size: 10, weight: 'bold', maxLines: 1 });
    paragraph(doc, item.evidence, RIGHT - 65, y, 65, { size: 10, color: TEAL, maxLines: 1 });
    y += 12;
  });
  appContext(doc, model, Math.max(213, y + 10));

  y = header(doc, model, tr('What to do next'));
  paragraph(doc, tr('Choose one change. Review it after your next normal work session.'), LEFT, y, WIDTH,
    { size: 11, color: MUTED, maxLines: 2 });
  y += 20;
  model.editorial.actions.forEach((action, index) => {
    if (index) line(doc, y - 7);
    y = paragraph(doc, action.title, LEFT, y, WIDTH, { size: 13, weight: 'bold', leading: 6, maxLines: 2 });
    if (action.evidence) y = paragraph(doc, action.evidence, LEFT, y + 2, WIDTH, { size: 9, color: TEAL, maxLines: 2 });
    y = paragraph(doc, action.body, LEFT, y + 4, WIDTH, { size: 10.5, leading: 5.5, maxLines: 4 }) + 17;
  });
  line(doc, 246);
  sectionTitle(doc, tr('How to read this report'), 255);
  paragraph(doc, tr('This covers recorded activity, not your whole working day.'), LEFT, 264, WIDTH,
    { size: 8.5, color: MUTED, maxLines: 1 });
  paragraph(doc, model.progress.method, LEFT, 270, WIDTH, { size: 8, leading: 3.9, color: MUTED, maxLines: 3 });
  footers(doc);
  return doc;
}
