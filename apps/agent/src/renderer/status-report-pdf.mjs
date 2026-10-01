import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, getLocale, getLanguage, initializeLocalization } from './i18n.mjs';
const PAGE_W = 210;
const LEFT = 18;
const RIGHT = PAGE_W - LEFT;
const WIDTH = RIGHT - LEFT;
const BOTTOM = 277;

const INK = [31, 30, 43];
const MUTED = [92, 91, 108];
const PURPLE = [112, 72, 196];
const RULE = [225, 221, 231];

function continuationPage(doc) {
  doc.addPage();
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(8);
  doc.setTextColor(...PURPLE);
  doc.text('FlowSight', LEFT, 16);
  doc.setDrawColor(...RULE);
  doc.line(LEFT, 20, RIGHT, 20);
  return 30;
}

function ensureSpace(doc, y, needed) {
  return y + needed > BOTTOM ? continuationPage(doc) : y;
}

function textLines(doc, value, width) {
  return doc.splitTextToSize(String(value ?? ''), width);
}

function drawParagraph(doc, value, y, { x = LEFT, width = WIDTH, size = 9, leading = 4.8, color = INK, weight = 'normal' } = {}) {
  doc.setFont('helvetica', weight);
  doc.setFontSize(size);
  for (const line of textLines(doc, value, width)) {
    y = ensureSpace(doc, y, leading);
    // A page break draws the continuation header, so restore the paragraph style.
    doc.setFont('helvetica', weight);
    doc.setFontSize(size);
    doc.setTextColor(...color);
    doc.text(line, x, y);
    y += leading;
  }
  return y;
}

function drawSection(doc, title, y, note = '', minContentHeight = 8) {
  y = ensureSpace(doc, y, (note ? 20 : 15) + minContentHeight);
  doc.setDrawColor(...RULE);
  doc.line(LEFT, y, RIGHT, y);
  y += 8;
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(12);
  doc.setTextColor(...INK);
  doc.text(title, LEFT, y);
  y += 6;
  if (note) {
    y = drawParagraph(doc, note, y, { size: 8, leading: 4, color: MUTED }) + 2;
  }
  return y;
}

function drawNumberedItems(doc, items, y) {
  if (!items.length) {
    return drawParagraph(doc, tr('No specific next move is supported by this period yet. Keep tracking to build a baseline.'), y, { color: MUTED }) + 4;
  }
  items.forEach((item, index) => {
    y = ensureSpace(doc, y, 14);
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(9);
    doc.setTextColor(...PURPLE);
    doc.text(String(index + 1).padStart(2, '0'), LEFT, y);
    y = drawParagraph(doc, item, y, { x: LEFT + 12, width: WIDTH - 12, size: 9.4, leading: 5 }) + 4;
  });
  return y;
}

function drawDistractionApps(doc, distractions, y) {
  if (!distractions.apps.length) {
    return drawParagraph(doc, distractions.message, y, { size: 8.8, leading: 4.6, color: MUTED }) + 4;
  }
  for (const [index, app] of distractions.apps.entries()) {
    y = ensureSpace(doc, y, 35);
    if (index) {
      doc.setDrawColor(...RULE);
      doc.line(LEFT, y - 3, RIGHT, y - 3);
      y += 2;
    }
    const headingY = y;
    const printableName = /^[\u0020-\u00FF]+$/.test(app.appName)
      ? app.appName
      : tr('App name unavailable in this PDF font - see on-screen report');
    const printableAdvice = printableName === app.appName
      ? app.advice
      : app.advice.replaceAll(app.appName, tr('this app'));
    const printableObserved = printableName === app.appName
      ? app.observed
      : app.observed.replaceAll(app.appName, tr('this app'));
    y = drawParagraph(doc, printableName, y, {
      x: LEFT, width: WIDTH - 32, size: 9.5, leading: 4.9, weight: 'bold',
    });
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(8.2);
    doc.setTextColor(...PURPLE);
    doc.text(app.duration, RIGHT, headingY, { align: 'right' });
    y += 1;
    doc.setFillColor(238, 235, 244);
    doc.roundedRect(LEFT, y, WIDTH, 2.2, 1, 1, 'F');
    doc.setFillColor(...PURPLE);
    doc.roundedRect(LEFT, y, Math.max(2, app.percentOfTop / 100 * WIDTH), 2.2, 1, 1, 'F');
    y += 7;
    y = drawParagraph(doc, printableObserved, y, { size: 8.5, leading: 4.4 }) + 1;
    y = drawParagraph(doc, formatMessage`Next session: ${printableAdvice}`, y, {
      size: 8.5, leading: 4.4, color: MUTED,
    }) + 5;
  }
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(7.5);
  y = ensureSpace(doc, y, textLines(doc, distractions.caveat, WIDTH).length * 4 + 3);
  return drawParagraph(doc, distractions.caveat, y, {
    size: 7.5, leading: 4, color: MUTED,
  }) + 3;
}

function drawSimpleItems(doc, items, y, emptyLabel) {
  if (!items.length) return drawParagraph(doc, emptyLabel, y, { size: 8.7, color: MUTED }) + 2;
  for (const item of items) {
    y = ensureSpace(doc, y, 8);
    doc.setFillColor(...PURPLE);
    doc.circle(LEFT + 1, y - 1, 0.8, 'F');
    y = drawParagraph(doc, item, y, { x: LEFT + 5, width: WIDTH - 5, size: 8.8, leading: 4.7 }) + 2;
  }
  return y + 1;
}

function drawMiniHeading(doc, title, y) {
  y = ensureSpace(doc, y, 16);
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(9);
  doc.setTextColor(...INK);
  doc.text(title, LEFT, y);
  return y + 5;
}

function drawDayChart(doc, days, y) {
  y = ensureSpace(doc, y, days.length ? 50 : 16);
  y = drawMiniHeading(doc, tr('Activity by day'), y);
  if (!days.length) return drawParagraph(doc, tr('No dated activity available.'), y, { color: MUTED }) + 5;
  y = ensureSpace(doc, y, 43);
  const gap = 4;
  const barW = (WIDTH - (days.length - 1) * gap) / days.length;
  const max = Math.max(1, ...days.map((day) => day.seconds));
  const chartTop = y + 4;
  const chartBottom = chartTop + 25;
  days.forEach((day, index) => {
    const x = LEFT + index * (barW + gap);
    const height = day.seconds ? Math.max(1.5, day.seconds / max * 25) : 0;
    doc.setFillColor(238, 235, 244);
    doc.roundedRect(x, chartTop, barW, 25, 1, 1, 'F');
    if (height) {
      doc.setFillColor(...PURPLE);
      doc.roundedRect(x, chartBottom - height, barW, height, 1, 1, 'F');
    }
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(days.length > 14 ? 5.5 : 7);
    doc.setTextColor(...INK);
    doc.text(day.label, x + barW / 2, chartBottom + 5, { align: 'center' });
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(days.length > 14 ? 5 : 6.5);
    doc.setTextColor(...MUTED);
    doc.text(day.displayDuration, x + barW / 2, chartBottom + 9, { align: 'center' });
  });
  return chartBottom + 15;
}

function drawCategoryChart(doc, categories, y) {
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(8);
  const estimatedHeight = 10 + categories.slice(0, 6).reduce((sum, category) =>
    sum + Math.max(8, textLines(doc, category.label, 45).length * 4.1 + 1) + 2, 0);
  y = ensureSpace(doc, y, Math.min(estimatedHeight, 220));
  y = drawMiniHeading(doc, tr('Time by category'), y);
  if (!categories.length) return drawParagraph(doc, tr('No category time recorded.'), y, { color: MUTED }) + 5;
  for (const category of categories.slice(0, 6)) {
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(8);
    const labelLines = textLines(doc, category.label, 45);
    const height = Math.max(8, labelLines.length * 4.1 + 1);
    if (height > 45) {
      y = drawParagraph(doc, category.label, y, { size: 8, leading: 4.1 }) + 1;
      y = ensureSpace(doc, y, 10);
      doc.setFillColor(238, 235, 244);
      doc.roundedRect(LEFT, y, WIDTH - 30, 5, 1, 1, 'F');
      if (category.percent > 0) {
        doc.setFillColor(...PURPLE);
        doc.roundedRect(LEFT, y, Math.max(2, category.percent / 100 * (WIDTH - 30)), 5, 1, 1, 'F');
      }
      doc.setFont('helvetica', 'bold');
      doc.setFontSize(8);
      doc.setTextColor(...INK);
      doc.text(category.displayDuration, RIGHT, y + 4, { align: 'right' });
      y += 11;
      continue;
    }
    y = ensureSpace(doc, y, height + 2);
    doc.setTextColor(...INK);
    labelLines.forEach((line, index) => doc.text(line, LEFT, y + 3 + index * 4.1));
    doc.setFillColor(238, 235, 244);
    doc.roundedRect(LEFT + 50, y, 96, 5, 1, 1, 'F');
    if (category.percent > 0) {
      doc.setFillColor(...PURPLE);
      doc.roundedRect(LEFT + 50, y, Math.max(2, category.percent / 100 * 96), 5, 1, 1, 'F');
    }
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(8);
    doc.text(category.displayDuration, RIGHT, y + 4, { align: 'right' });
    y += height + 2;
  }
  return y + 3;
}

function drawEvidenceColumns(doc, model, y) {
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(7.2);
  const compact = model.days.length === 7
    && model.categories.slice(0, 6).every((category) => doc.getTextWidth(category.label) <= 30);
  if (!compact) {
    y = drawDayChart(doc, model.days, y);
    return drawCategoryChart(doc, model.categories, y);
  }

  const panelHeight = Math.max(44, 18 + Math.min(model.categories.length, 6) * 6.2);
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(8.5);
  const captionHeight = model.timelineCaption
    ? textLines(doc, model.timelineCaption, WIDTH).length * 4.5 + 6 : 0;
  y = ensureSpace(doc, y, panelHeight + captionHeight);
  const gap = 12;
  const columnW = (WIDTH - gap) / 2;
  const rightX = LEFT + columnW + gap;
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(9);
  doc.setTextColor(...INK);
  doc.text(tr('Activity by day'), LEFT, y);
  doc.text(tr('Time by category'), rightX, y);

  const chartTop = y + 10;
  const chartBottom = chartTop + 25;
  const dayGap = 2;
  const dayW = (columnW - dayGap * 6) / 7;
  const maxSeconds = Math.max(1, ...model.days.map((day) => day.seconds));
  model.days.forEach((day, index) => {
    const x = LEFT + index * (dayW + dayGap);
    const height = day.seconds ? Math.max(1.5, day.seconds / maxSeconds * 25) : 0;
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(5.8);
    doc.setTextColor(...MUTED);
    doc.text(day.displayDuration, x + dayW / 2, chartTop - 2, { align: 'center' });
    doc.setFillColor(238, 235, 244);
    doc.roundedRect(x, chartTop, dayW, 25, 1, 1, 'F');
    if (height) {
      doc.setFillColor(...PURPLE);
      doc.roundedRect(x, chartBottom - height, dayW, height, 1, 1, 'F');
    }
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(6.2);
    doc.setTextColor(...INK);
    doc.text(day.label, x + dayW / 2, chartBottom + 4.5, { align: 'center' });
  });

  if (!model.categories.length) {
    drawParagraph(doc, tr('No category time recorded.'), chartTop + 5, {
      x: rightX, width: columnW, size: 8, leading: 4.2, color: MUTED,
    });
  }
  model.categories.slice(0, 6).forEach((category, index) => {
    const rowY = chartTop + 2 + index * 7.3;
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(7.2);
    doc.setTextColor(...INK);
    doc.text(category.label, rightX, rowY + 1);
    doc.setFillColor(238, 235, 244);
    doc.roundedRect(rightX + 32, rowY - 2, 31, 4, 1, 1, 'F');
    if (category.percent > 0) {
      doc.setFillColor(...PURPLE);
      doc.roundedRect(rightX + 32, rowY - 2, Math.max(1.5, category.percent / 100 * 31), 4, 1, 1, 'F');
    }
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(7);
    doc.text(category.displayDuration, rightX + columnW, rowY + 1, { align: 'right' });
  });
  return y + panelHeight;
}

function drawReportHeader(doc, model) {
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(9);
  doc.setTextColor(...PURPLE);
  doc.text('FlowSight', LEFT, 17);
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(7.5);
  doc.setTextColor(...MUTED);
  doc.text(model.generatedAt, RIGHT, 17, { align: 'right' });
  doc.setDrawColor(...RULE);
  doc.line(LEFT, 23, RIGHT, 23);
  doc.setFont('helvetica', 'bold');
  doc.setFontSize(23);
  doc.setTextColor(...INK);
  doc.text(model.title, LEFT, 38);
  doc.setFont('helvetica', 'normal');
  doc.setFontSize(10);
  doc.setTextColor(...MUTED);
  doc.text(model.period || tr('Current period'), LEFT, 47);
  return drawParagraph(doc, `${model.userName}  ·  ${model.status}`, 56, {
    size: 8.2, leading: 4.4, color: MUTED,
  }) + 8;
}

function drawMetrics(doc, model, y) {
  y = ensureSpace(doc, y, 30);
  const values = [
    [model.totalHours + 'h', tr('TRACKED TIME')],
    [model.focusHours + 'h', formatMessage`SUSTAINED FOCUS · ${model.focusSessions} BLOCKS`],
    [`${model.activeDays}/${model.periodDays}`, tr('DAYS WITH ACTIVITY')],
  ];
  const columnW = WIDTH / 3;
  values.forEach(([value, label], index) => {
    const x = LEFT + index * columnW;
    if (index) {
      doc.setDrawColor(...RULE);
      doc.line(x - 5, y, x - 5, y + 21);
    }
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(18);
    doc.setTextColor(...INK);
    doc.text(value, x, y + 10);
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(6.8);
    doc.setTextColor(...MUTED);
    doc.text(textLines(doc, label, columnW - 7), x, y + 17);
  });
  return y + 27;
}

function drawWorkAreas(doc, rows, y) {
  if (!rows.length) return drawParagraph(doc, tr('No work-area detail was generated.'), y, { color: MUTED }) + 4;
  for (const row of rows) {
    y = ensureSpace(doc, y, 15);
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(9);
    doc.setTextColor(...INK);
    doc.setFontSize(8);
    const compactStatus = doc.getTextWidth(row.status) <= 43;
    const headingY = y;
    y = drawParagraph(doc, row.element, y, {
      width: compactStatus ? WIDTH - 47 : WIDTH,
      size: 9, leading: 4.8, weight: 'bold',
    });
    if (compactStatus) {
      doc.setFont('helvetica', 'normal');
      doc.setFontSize(8);
      doc.setTextColor(...PURPLE);
      doc.text(row.status, RIGHT, headingY, { align: 'right' });
    } else {
      y = drawParagraph(doc, row.status, y, { size: 8, leading: 4.2, color: PURPLE });
    }
    if (row.notes) y = drawParagraph(doc, row.notes, y, { size: 8.6, leading: 4.6 });
    if (row.owner && row.owner.toLowerCase() !== 'self') {
      y = drawParagraph(doc, formatMessage`Owner: ${row.owner}`, y, { size: 7.5, leading: 4, color: MUTED });
    }
    y += 1;
  }
  return y;
}

function drawCompactDetails(doc, model, y) {
  const leftItems = [...model.observedWork, ...model.highlights];
  const rightItems = [...model.knownIssues, ...model.potentialRisks];
  if (leftItems.length > 4 || rightItems.length > 4
      || [...leftItems, ...rightItems].some((item) => item.length > 160)) return null;

  const gap = 12;
  const columnW = (WIDTH - gap) / 2;
  const rightX = LEFT + columnW + gap;
  const itemHeight = (items, emptyLabel = '') => {
    const list = items.length ? items : [emptyLabel];
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(8.3);
    return list.reduce((sum, item) => sum + textLines(doc, item, columnW - 7).length * 4.3 + 2, 0);
  };
  const leftHeight = 15 + itemHeight(model.observedWork, tr('No labelled work observed.'))
    + (model.highlights.length ? 7 + itemHeight(model.highlights) : 0);
  const rightHeight = 15 + 7 + itemHeight(model.knownIssues, tr('None flagged.'))
    + 7 + itemHeight(model.potentialRisks, tr('None flagged.'));
  const height = Math.max(leftHeight, rightHeight) + 5;
  if (height > 190) return null;
  y = ensureSpace(doc, y, height);
  doc.setDrawColor(...RULE);
  doc.line(LEFT, y, RIGHT, y);

  const title = (label, x) => {
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(12);
    doc.setTextColor(...INK);
    doc.text(label, x, y + 8);
  };
  const subheading = (label, x, at) => {
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(8.5);
    doc.setTextColor(...INK);
    doc.text(label, x, at);
    return at + 6;
  };
  const items = (values, fallback, x, at) => {
    const list = values.length ? values : [fallback];
    for (const item of list) {
      doc.setFont('helvetica', 'normal');
      doc.setFontSize(8.3);
      doc.setTextColor(...INK);
      const lines = textLines(doc, item, columnW - 7);
      doc.setFillColor(...PURPLE);
      doc.circle(x + 1, at - 1, 0.7, 'F');
      lines.forEach((line) => {
        doc.text(line, x + 5, at);
        at += 4.3;
      });
      at += 2;
    }
    return at;
  };

  title(tr('Work observed'), LEFT);
  title(tr('Watchpoints'), rightX);
  let leftY = items(model.observedWork, tr('No labelled work observed.'), LEFT, y + 16);
  if (model.highlights.length) {
    leftY = subheading(tr('Highlights'), LEFT, leftY + 1);
    leftY = items(model.highlights, '', LEFT, leftY);
  }
  let rightY = subheading(tr('Known issues'), rightX, y + 16);
  rightY = items(model.knownIssues, tr('None flagged.'), rightX, rightY);
  rightY = subheading(tr('Potential risks'), rightX, rightY + 1);
  rightY = items(model.potentialRisks, tr('None flagged.'), rightX, rightY);
  return Math.max(leftY, rightY) + 3;
}

function drawCompactLessons(doc, lessons, y) {
  if (lessons.length !== 2) return null;
  const gap = 12;
  const columnW = (WIDTH - gap) / 2;
  const rightX = LEFT + columnW + gap;
  const measured = lessons.map((lesson) => {
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(8.8);
    const title = textLines(doc, lesson.title, columnW);
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(8.3);
    const body = textLines(doc, lesson.body, columnW);
    return { title, body, height: title.length * 4.7 + body.length * 4.4 + 3 };
  });
  const height = Math.max(...measured.map((lesson) => lesson.height));
  if (height > 40) return null;
  y = drawSection(doc, tr('What this period taught us'), y, '', height);
  measured.forEach((lesson, index) => {
    const x = index ? rightX : LEFT;
    let lineY = y;
    doc.setFont('helvetica', 'bold');
    doc.setFontSize(8.8);
    doc.setTextColor(...INK);
    lesson.title.forEach((line) => {
      doc.text(line, x, lineY);
      lineY += 4.7;
    });
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(8.3);
    doc.setTextColor(...INK);
    lesson.body.forEach((line) => {
      doc.text(line, x, lineY);
      lineY += 4.4;
    });
  });
  return y + height;
}

function addFooters(doc, model) {
  const pageCount = doc.internal.getNumberOfPages();
  for (let page = 1; page <= pageCount; page++) {
    doc.setPage(page);
    doc.setDrawColor(...RULE);
    doc.line(LEFT, 284, RIGHT, 284);
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(7);
    doc.setTextColor(...MUTED);
    doc.text(model.aiPowered
      ? tr('Local data · Local AI narrative · Interpret with context')
      : tr('Local data · Rule-based narrative · Interpret with context'), LEFT, 290);
    doc.text(`${page} / ${pageCount}`, RIGHT, 290, { align: 'right' });
  }
}

export function renderStatusReportPdf(doc, model) {
  let y = drawReportHeader(doc, model);
  y = drawParagraph(doc, model.summary, y, { size: 10, leading: 5.5 }) + 7;
  y = drawMetrics(doc, model, y);

  y = drawSection(doc, tr('What to do next'), y, tr('Actions suggested by the recorded evidence'), 16);
  y = drawNumberedItems(doc, model.actions, y);

  y = drawSection(doc, tr('Attention detours'), y,
    tr('Observed visits and returns between work screens'),
    model.distractions.apps.length ? 35 : 8);
  y = drawDistractionApps(doc, model.distractions, y);

  y = drawSection(doc, model.evidenceTitle, y, tr('Recorded time, not a productivity score'), 54);
  y = drawEvidenceColumns(doc, model, y);
  if (model.timelineCaption) y = drawParagraph(doc, model.timelineCaption, y, { size: 8.5, leading: 4.5, color: MUTED }) + 6;

  y = drawSection(doc, tr('How to read the signal'), y, '', 20);
  y = drawParagraph(doc, model.healthNotes || tr('No additional interpretation was generated.'), y, { size: 9, leading: 4.8 }) + 4;
  if (model.focusTarget) y = drawParagraph(doc, formatMessage`Focus target: ${model.focusTarget}`, y, { size: 8.8, leading: 4.7 }) + 4;

  y = drawSection(doc, tr('Work-area detail'), y, tr('Specific observations behind the review'), 16);
  y = drawWorkAreas(doc, model.breakdown, y);

  const compactDetailsEnd = drawCompactDetails(doc, model, y);
  if (compactDetailsEnd == null) {
    y = drawSection(doc, tr('Work observed'), y, '', 16);
    y = drawSimpleItems(doc, model.observedWork, y, tr('No labelled work was observed.'));
    if (model.highlights.length) {
      doc.setFont('helvetica', 'normal');
      doc.setFontSize(8.8);
      const highlightHeight = 9 + model.highlights.reduce((sum, item) =>
        sum + textLines(doc, item, WIDTH - 5).length * 4.7 + 2, 0);
      y = ensureSpace(doc, y, Math.min(highlightHeight, 190));
      y = drawMiniHeading(doc, tr('Highlights'), y);
      y = drawSimpleItems(doc, model.highlights, y, '');
    }

    y = ensureSpace(doc, y, 55);
    y = drawSection(doc, tr('Watchpoints'), y, '', 30);
    y = drawMiniHeading(doc, tr('Known issues'), y);
    y = drawSimpleItems(doc, model.knownIssues, y, tr('None flagged.'));
    y = drawMiniHeading(doc, tr('Potential risks'), y);
    y = drawSimpleItems(doc, model.potentialRisks, y, tr('None flagged.'));
  } else {
    y = compactDetailsEnd;
  }

  const compactLessonsEnd = drawCompactLessons(doc, model.lessons, y);
  if (compactLessonsEnd == null) {
    doc.setFont('helvetica', 'normal');
    doc.setFontSize(8.8);
    const lessonHeight = 22 + model.lessons.reduce((sum, lesson) => {
      doc.setFont('helvetica', 'bold');
      doc.setFontSize(9);
      const titleHeight = textLines(doc, lesson.title, WIDTH).length * 4.8;
      doc.setFont('helvetica', 'normal');
      doc.setFontSize(8.8);
      return sum + titleHeight + textLines(doc, lesson.body, WIDTH).length * 4.7 + 4;
    }, 0);
    y = ensureSpace(doc, y, Math.min(lessonHeight, 190));
    y = drawSection(doc, tr('What this period taught us'), y, '', 15);
    if (!model.lessons.length) {
      y = drawParagraph(doc, model.lessonEmptyMessage, y, { color: MUTED }) + 3;
    } else {
      for (const lesson of model.lessons) {
        y = ensureSpace(doc, y, 12);
        y = drawParagraph(doc, lesson.title, y, { size: 9, leading: 4.8, weight: 'bold' });
        y = drawParagraph(doc, lesson.body, y, { size: 8.8, leading: 4.7 }) + 4;
      }
    }
  } else {
    y = compactLessonsEnd;
  }
  addFooters(doc, model);
  return doc;
}
