import { setLanguagePreference, t } from './i18n.mjs';
setLanguagePreference('en',{persist:false});
import test from 'node:test';
import assert from 'node:assert/strict';
import { jsPDF } from 'jspdf';

import { createStatusReportViewModel, renderStatusReportHtml } from './status-report.mjs';
import { renderStatusReportPdf } from './status-report-pdf.mjs';

test('Spanish screen and PDF use the paired narrative while preserving user names and metrics', () => {
  const payload={report:{executive_overview:'Recorded work.',overall_health:'Sustained blocks observed'},localized_report:{es:{executive_overview:'Trabajo registrado.',overall_health:'Bloques sostenidos observados',recommendations:['Revisa el siguiente bloque.']}},local_data:{period_start:'2026-10-01',period_end:'2026-10-01',total_seconds:3600,deep_focus_seconds:1800,deep_focus_sessions:1,active_days:1,category_breakdown:[{category:'Coding',total_seconds:3600}]}};
  setLanguagePreference('es',{persist:false});
  try {
    const model=createStatusReportViewModel(payload,{userName:'Review · María'});
    assert.equal(model.summary,'Trabajo registrado.');assert.equal(model.statusTone,'positive');assert.equal(model.userName,'Review · María');assert.equal(model.categories[0].label,'Programación');
    const output=renderStatusReportHtml(model);assert.match(output,/Tiempo registrado/);assert.match(output,/Review · María/);assert.doesNotMatch(output,/>Tracked time</);
    const doc=new jsPDF(),drawn=[];const original=doc.text.bind(doc);doc.text=(value,...args)=>{drawn.push(String(value));return original(value,...args);};renderStatusReportPdf(doc,model);
    assert.ok(drawn.some(line=>line.includes('Ajusta tu tiempo a tus prioridades')));assert.ok(drawn.some(line=>line.includes(t('Sustained focus'))));assert.ok(drawn.some(line=>line.includes('Review · María')));
    assert.match(output, /Revisa el siguiente bloque\./); // Paired source remains in the disclosure.
  } finally {setLanguagePreference('en',{persist:false});}
});

// Synthetic activity only: these numbers are fixtures, not a customer report.
test('focus grade, prior grade and improvements lead the screen and PDF and use identical Spanish values', () => {
  const payload = syntheticReport();
  Object.assign(payload.local_data, { focus_eligible_seconds: 18000, distraction_seconds: 1800, focus_semantics: { longest_focus_seconds: 2700 },
    prior_period: { period_start: '2026-09-14', period_end: '2026-09-20', total_seconds: 36000, focus_eligible_seconds: 18000, deep_focus_seconds: 7200, distraction_seconds: 3600, longest_focus_seconds: 1800 } });
  setLanguagePreference('es', { persist: false });
  try {
    const model = createStatusReportViewModel(payload);
    const output = renderStatusReportHtml(model);
    assert.ok(output.indexOf('sr-grade-number') < output.indexOf('sr-overview'));
    assert.match(output, /6,9/); assert.match(output, /5,3/); assert.match(output, /\+1,6/);
    assert.match(output, /En qué has mejorado/); assert.match(output, /En qué puedes mejorar/);
    assert.doesNotMatch(output, />Previous</);
    const doc = new jsPDF(), drawn = []; const original = doc.text.bind(doc);
    doc.text = (value, ...args) => { drawn.push(String(value)); return original(value, ...args); };
    renderStatusReportPdf(doc, model);
    assert.ok(drawn.some(value => value.includes('6,9/10')));
    assert.ok(drawn.some(value => value.includes('5,3/10')));
    assert.ok(drawn.some(value => value.includes('+1,6')));
    assert.ok(drawn.indexOf('En qué has mejorado') < drawn.indexOf(t('What to do next')));
    assert.ok(drawn.some(value => value.includes('Más tiempo en foco sostenido')));
  } finally { setLanguagePreference('en', { persist: false }); }
});

function syntheticReport() {
  return {
    generated_at: '2026-09-27 12:00',
    ai_powered: true,
    local_data: {
      period_start: '2026-09-21',
      period_end: '2026-09-27',
      period_days: 7,
      total_seconds: 36_000,
      deep_focus_seconds: 10_800,
      deep_focus_sessions: 2,
      active_days: 2,
      daily_totals: [
        { date: '2026-09-21', total_seconds: 14_400 },
        { date: '2026-09-24', total_seconds: 21_600 },
      ],
      category_breakdown: [
        { category: 'Planning', total_seconds: 21_600 },
        { category: 'Build', total_seconds: 14_400 },
      ],
    },
    report: {
      executive_overview: 'Ten hours were recorded across two days.',
      overall_health: 'Fragmented eligible work',
      health_notes: 'Two sustained blocks were observed.',
      timeline_caption: 'Activity was concentrated on Thursday.',
      recommendations: ['Reserve a planning block.', 'Review the observed short breaks.'],
      health_breakdown: [{ element: 'Planning', status: 'Observed', owner_team: 'Self', notes: 'Six hours recorded.' }],
      known_issues: ['Coverage is sparse.'],
      potential_risks: ['This period may under-represent work.'],
      observed_work: ['Planning tasks recorded.'],
      work_progress: ['Build work increased.'],
      lessons_learned: [{ title: 'Label work', body: 'Labels improved continuity.' }],
    },
  };
}

test('the review and PDF model use the report period, including days with zero recorded time', () => {
  const model = createStatusReportViewModel(syntheticReport(), { userName: 'Sample user' });
  assert.equal(model.days.length, 7);
  assert.equal(model.days[1].date, '2026-09-22');
  assert.equal(model.days[1].seconds, 0);
  assert.equal(model.days[3].seconds, 21_600);
  assert.equal(model.categories[0].percent, 60);
  assert.equal(model.statusTone, 'attention');
  assert.equal(model.activeDays, 2);
});

test('visual evidence leads decisions and untrusted source text stays escaped in a closed disclosure', () => {
  const payload = syntheticReport();
  payload.report.recommendations[0] = 'Review <script>alert(1)</script> breaks.';
  const html = renderStatusReportHtml(createStatusReportViewModel(payload));
  assert.ok(html.indexOf('The week in view') < html.indexOf('What to do next'));
  assert.ok(html.includes('&lt;script&gt;'));
  assert.ok(!html.includes('<script>'));
  assert.ok(html.includes('2026-09-22: 0 min'));
  assert.match(html, /<details class="sr-report-details"><summary>/);
  assert.doesNotMatch(html, /<details class="sr-report-details" open/);
});

test('an empty period does not invent positive signals or actions', () => {
  const payload = syntheticReport();
  payload.local_data.total_seconds = 0;
  payload.local_data.deep_focus_seconds = 0;
  payload.local_data.active_days = 0;
  payload.local_data.daily_totals = [];
  payload.local_data.category_breakdown = [];
  payload.report.recommendations = [];
  payload.report.overall_health = 'No sustained-work signal';
  const model = createStatusReportViewModel(payload);
  assert.equal(model.empty, true);
  assert.equal(model.statusTone, 'neutral');
  assert.equal(model.days.length, 7);
  assert.equal(model.actions.length, 0);
  assert.equal(model.lessons.length, 0);
  assert.ok(renderStatusReportHtml(model).includes('No specific next move is supported'));
  assert.ok(renderStatusReportHtml(model).includes('not enough evidence to draw a lesson'));
});

test('a one-day report uses daily headings and shows short categories in minutes', () => {
  const payload = syntheticReport();
  payload.local_data.period_start = '2026-09-29';
  payload.local_data.period_end = '2026-09-29';
  payload.local_data.period_days = 1;
  payload.local_data.total_seconds = 3724;
  payload.local_data.daily_totals = [{ date: '2026-09-29', total_seconds: 3724 }];
  payload.local_data.category_breakdown = [
    { category: 'Research', total_seconds: 3600 },
    { category: 'Browsing', total_seconds: 124 },
  ];
  const model = createStatusReportViewModel(payload);
  assert.equal(model.title, 'Daily work review');
  assert.equal(model.evidenceTitle, 'The day in view');
  assert.equal(model.period, '2026-09-29');
  assert.equal(model.categories[1].displayDuration, '2.1 min');
  const html = renderStatusReportHtml(model);
  assert.ok(html.includes('The day in view'));
  assert.ok(html.includes('2.1 min'));
  const doc = new jsPDF();
  const drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    drawn.push(String(value));
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.ok(drawn.some((line) => line.includes('The day in view')));
  assert.ok(drawn.some((line) => line.includes('2.1 min')));
});

test('missing AI lessons recover in the source disclosure while the main review follows chart data', () => {
  const payload = syntheticReport();
  payload.report.lessons_learned = [{ title: '', body: 'Incomplete AI item' }];
  const model = createStatusReportViewModel(payload);

  assert.ok(model.lessons.some((lesson) => lesson.body.includes('6.0 h (60% of tracked time)')));
  assert.ok(model.lessons.some((lesson) => lesson.body.includes('2 of 7 days')));
  const html = renderStatusReportHtml(model);
  assert.ok(html.includes('The work mix had a clear centre'));
  assert.ok(!html.includes('No lessons were generated'));

  const doc = new jsPDF();
  const drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    drawn.push(String(value));
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.ok(drawn.some((line) => line.includes('Planning')));
  assert.ok(drawn.some((line) => line.includes('6.0 h')));
  assert.ok(!drawn.some((line) => line.includes('No lessons were generated')));

  delete payload.report.lessons_learned;
  assert.ok(createStatusReportViewModel(payload).lessons.length > 0);
});

test('named destinations and work-interleaved revisits appear in the review and PDF', () => {
  const payload = syntheticReport();
  payload.local_data.distraction_app_analysis = {
    qualifying_episodes: 5,
    qualifying_seconds: 2400,
    attributed_seconds: 2160,
    unattributed_seconds: 240,
    other_app_seconds: 0,
    apps: [
      {
        app_name: 'Browser <A>', seconds: 1260, episodes: 3, days: 2,
        transitions_from_focus: 2,
        daily_seconds: [
          { date: '2026-09-21', seconds: 900 },
          { date: '2026-09-24', seconds: 360 },
        ],
      },
      {
        app_name: 'Video player', seconds: 900, episodes: 2, days: 2,
        transitions_from_focus: 0,
        daily_seconds: [
          { date: '2026-09-22', seconds: 540 },
          { date: '2026-09-24', seconds: 360 },
        ],
      },
    ],
    detours: [
      {
        label: 'Apple <Music>', kind: 'music', seconds: 1260, visits: 3, days: 2,
        work_interleaved_revisits: 1,
        daily_visits: [
          { date: '2026-09-21', seconds: 900, visits: 1, work_interleaved_revisits: 0, shortest_revisit_minutes: null },
          { date: '2026-09-24', seconds: 360, visits: 2, work_interleaved_revisits: 1, shortest_revisit_minutes: 12, observed_at: ['11:21', '11:33'] },
        ],
      },
      {
        label: 'YouTube', kind: 'video', seconds: 900, visits: 2, days: 2,
        work_interleaved_revisits: 0,
        daily_visits: [
          { date: '2026-09-22', seconds: 540, visits: 1, work_interleaved_revisits: 0, shortest_revisit_minutes: null },
          { date: '2026-09-24', seconds: 360, visits: 1, work_interleaved_revisits: 0, shortest_revisit_minutes: null },
        ],
      },
    ],
  };
  payload.report.lessons_learned.push({ title: 'Use the calendar', body: 'Compare shorter days with planned coordination.' });
  const model = createStatusReportViewModel(payload);
  assert.equal(model.distractions.state, 'ready');
  assert.equal(model.distractions.apps[0].duration, '21 min');
  assert.match(model.distractions.apps[0].observed, /foreground sightings around 11:21 and 11:33/);
  assert.match(model.distractions.apps[0].observed, /Work screens appeared between sightings; Apple <Music> reappeared once, with the shortest interval 12 min/);
  assert.match(model.distractions.apps[0].advice, /Choose a playlist/);
  assert.ok(!model.distractions.caveat.includes('Privacy-excluded'));

  const html = renderStatusReportHtml(model);
  assert.ok(html.indexOf('The week in view') < html.indexOf('App changes, with context'));
  assert.ok(html.includes('Apple &lt;Music&gt;'));
  assert.ok(!html.includes('Apple <Music>'));
  assert.ok(html.includes('2026-09-24: 2 foreground sightings'));

  const doc = new jsPDF();
  const drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    drawn.push(String(value));
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.ok(drawn.some((line) => line.includes('App changes, with context')));
  assert.ok(drawn.some((line) => line.includes('Apple <Music>')));
  assert.ok(drawn.some((line) => line.includes('observations, not proof of distraction')));
  assert.ok(!drawn.some((line) => line.includes('11:21') || line.includes('Next session:')));
  assert.equal(doc.internal.getNumberOfPages(), 3);
});

test('older and empty destination evidence never fall back to a generic browser process', () => {
  const payload = syntheticReport();
  payload.local_data.category_breakdown.push({ category: 'Browsing', total_seconds: 900 });
  assert.equal(createStatusReportViewModel(payload).distractions.state, 'unavailable');

  payload.local_data.distraction_app_analysis = { unavailable: true };
  assert.match(createStatusReportViewModel(payload).distractions.message, /Try generating the report again/);

  payload.local_data.distraction_app_analysis = {
    qualifying_episodes: 0, qualifying_seconds: 0, attributed_seconds: 0,
    unattributed_seconds: 0, other_app_seconds: 0,
    apps: [{ app_name: 'Arc', seconds: 900 }], detours: [],
  };
  const model = createStatusReportViewModel(payload);
  assert.equal(model.distractions.state, 'none');
  const html = renderStatusReportHtml(model);
  assert.ok(html.includes('No recurring work-to-app return'));
  assert.ok(!html.includes('Arc'));
});

test('generic native apps receive advice that does not assume they are browser tabs', () => {
  const payload = syntheticReport();
  payload.local_data.distraction_app_analysis = {
    qualifying_episodes: 0, qualifying_seconds: 0, attributed_seconds: 0,
    unattributed_seconds: 0, other_app_seconds: 0, apps: [],
    detours: [
      { label: 'Slack', kind: 'communication', seconds: 120, visits: 2, days: 1,
        work_interleaved_revisits: 1, daily_visits: [
          { date: '2026-09-24', seconds: 120, visits: 2, work_interleaved_revisits: 1, shortest_revisit_minutes: 8, observed_at: ['09:02', '09:10'] },
        ] },
      { label: 'PixelNest', kind: 'other', seconds: 120, visits: 2, days: 1,
        work_interleaved_revisits: 1, daily_visits: [
          { date: '2026-09-24', seconds: 120, visits: 2, work_interleaved_revisits: 1, shortest_revisit_minutes: 8, observed_at: ['10:02', '10:10'] },
        ] },
    ],
  };
  const model = createStatusReportViewModel(payload);
  assert.equal(model.distractions.apps.length, 2);
  assert.match(model.distractions.apps[0].advice, /mute it for the next focus block/);
  assert.match(model.distractions.apps[1].advice, /belongs to the current task/);
  assert.ok(!model.distractions.apps.some((app) => app.advice.includes('tab')));
  const html = renderStatusReportHtml(model);
  assert.ok(html.includes('Slack'));
  assert.ok(html.includes('PixelNest'));
});

test('PDF marks app names its core font cannot render instead of silently corrupting them', () => {
  const payload = syntheticReport();
  payload.local_data.distraction_app_analysis = {
    qualifying_episodes: 1, qualifying_seconds: 180,
    attributed_seconds: 180, unattributed_seconds: 0, other_app_seconds: 0,
    apps: [],
    detours: [{
      label: '视频播放器', kind: 'video', seconds: 360, visits: 2, days: 1,
      work_interleaved_revisits: 1,
      daily_visits: [{ date: '2026-09-21', seconds: 360, visits: 2, work_interleaved_revisits: 1, shortest_revisit_minutes: 5 }],
    }],
  };
  const model = createStatusReportViewModel(payload);
  assert.ok(renderStatusReportHtml(model).includes('视频播放器'));
  const doc = new jsPDF();
  const drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    drawn.push(String(value));
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.ok(drawn.some((line) => line.includes('App name unavailable in this PDF font')));
  assert.ok(!drawn.some((line) => line.includes('视频播放器')));
});

test('app context and its first app stay together despite excessive generated recommendations', () => {
  for (const actionCount of [1, 3, 5, 7, 9]) {
    const payload = syntheticReport();
    payload.report.recommendations = Array.from({ length: actionCount }, (_, index) =>
      `Action ${index + 1}: ${'Review the next session before making a change. '.repeat(4)}`);
    payload.local_data.distraction_app_analysis = {
      qualifying_episodes: 1, qualifying_seconds: 180,
      attributed_seconds: 180, unattributed_seconds: 0, other_app_seconds: 0,
      apps: [],
      detours: [{
        label: 'Sample service', kind: 'video', seconds: 360, visits: 2, days: 1,
        work_interleaved_revisits: 1,
        daily_visits: [{ date: '2026-09-21', seconds: 360, visits: 2, work_interleaved_revisits: 1, shortest_revisit_minutes: 5 }],
      }],
    };
    const doc = new jsPDF();
    const pages = {};
    const originalText = doc.text.bind(doc);
    doc.text = (value, ...args) => {
      const line = String(value);
      if (line === 'App changes, with context' || line === 'Sample service') {
        pages[line] = doc.internal.getCurrentPageInfo().pageNumber;
      }
      return originalText(value, ...args);
    };
    renderStatusReportPdf(doc, createStatusReportViewModel(payload));
    assert.equal(pages['App changes, with context'], pages['Sample service']);
  }
});

test('PDF states app uncertainty once and does not expand verbose generated caveats', () => {
  const model = createStatusReportViewModel(syntheticReport());
  model.distractions = {
    apps: Array.from({ length: 3 }, (_, index) => ({
      appName: `Sample browser ${index + 1}`,
      duration: '20 min',
      percentOfTop: 100 - index * 20,
      observed: '20 min in two qualifying browsing episodes across two days.',
      advice: 'Move optional checks to a planned break.',
    })),
    caveat: 'APP_CAVEAT '.repeat(55),
  };
  const doc = new jsPDF();
  const caveatPages = new Set(), drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    drawn.push(String(value));
    if (String(value).includes('observations, not proof of distraction')) {
      caveatPages.add(doc.internal.getCurrentPageInfo().pageNumber);
    }
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.equal(caveatPages.size, 1);
  assert.ok(!drawn.some(value => value.includes('APP_CAVEAT')));
});

test('the reader review has three purposeful pages and both charts on the opening page', () => {
  const model = createStatusReportViewModel(syntheticReport());
  const doc = new jsPDF(), chartPages = [];
  const original = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    if (['Activity by day', 'Time by category'].includes(String(value))) chartPages.push(doc.internal.getCurrentPageInfo().pageNumber);
    return original(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.equal(doc.internal.getNumberOfPages(), 3);
  assert.deepEqual(chartPages, [1, 1]);
});

test('dense source copy stays available on screen without creating a long or technical PDF', () => {
  const payload = syntheticReport();
  const long = 'Review each observed block before changing the schedule. '.repeat(23);
  payload.report.recommendations = Array.from({ length: 5 }, (_, index) => `Action ${index + 1}: ${long} END_ACTION_${index + 1}`);
  payload.report.health_breakdown = Array.from({ length: 6 }, (_, index) => ({
    element: `Work area ${index + 1}`,
    status: 'Observed',
    notes: `${long} END_AREA_${index + 1}`,
  }));
  payload.report.lessons_learned = Array.from({ length: 4 }, (_, index) => ({ title: `Lesson ${index + 1}`, body: long }));
  payload.local_data.distraction_app_analysis = {
    qualifying_episodes: 10, qualifying_seconds: 6000,
    attributed_seconds: 6000, unattributed_seconds: 0, other_app_seconds: 0,
    apps: [],
    detours: Array.from({ length: 5 }, (_, index) => ({
      label: `Example destination ${index + 1}`, kind: 'video',
      seconds: 1200, visits: 2, days: 2, work_interleaved_revisits: 1,
      daily_visits: [
        { date: '2026-09-21', seconds: 600, visits: 1, work_interleaved_revisits: 0, shortest_revisit_minutes: null },
        { date: '2026-09-24', seconds: 600, visits: 1, work_interleaved_revisits: 0, shortest_revisit_minutes: null },
      ],
    })),
  };
  const model = createStatusReportViewModel(payload);
  const doc = new jsPDF({ compress: false });
  const drawn = [];
  const originalText = doc.text.bind(doc);
  doc.text = (value, x, y, ...rest) => {
    drawn.push({ value: String(value), y });
    assert.ok(y <= 291, `text baseline ${y} exceeded the page footer`);
    return originalText(value, x, y, ...rest);
  };
  renderStatusReportPdf(doc, model);
  assert.equal(doc.internal.getNumberOfPages(), 3);
  assert.ok(doc.output('arraybuffer').byteLength > 10_000);
  assert.ok(!drawn.some((entry) => entry.value.includes('END_ACTION_5') || entry.value.includes('END_AREA_6')));
  const screen = renderStatusReportHtml(model);
  assert.ok(screen.includes('END_ACTION_5') && screen.includes('END_AREA_6'));
});

test('the category visual accounts for the full distribution and uses one localized duration convention', () => {
  const payload = syntheticReport();
  payload.local_data.category_breakdown = Array.from({ length: 10 }, (_, index) => ({ category: `Area ${index}`, total_seconds: (10 - index) * 60 }));
  payload.local_data.total_seconds = 3300;
  setLanguagePreference('es', { persist: false });
  try {
    const model = createStatusReportViewModel(payload);
    assert.equal(model.editorial.chartCategories.length, 6);
    assert.equal(model.editorial.chartCategories.at(-1).label, 'Otras categorías');
    assert.equal(model.editorial.chartCategories.reduce((sum, row) => sum + row.seconds, 0), 3300);
    assert.equal(model.totalDuration, '55 min');
    assert.equal(model.days[0].displayDuration, '4,0 h');
    assert.equal(model.editorial.actions.length <= 3, true);
    assert.ok(model.editorial.actions.some(action => action.evidence.includes('Area 0')));
  } finally { setLanguagePreference('en', { persist: false }); }
});

test('empty reports give one honest page without a grade or decisions', () => {
  const payload = syntheticReport();
  Object.assign(payload.local_data, { total_seconds: 0, deep_focus_seconds: 0, active_days: 0, daily_totals: [], category_breakdown: [] });
  const model = createStatusReportViewModel(payload);
  const doc = renderStatusReportPdf(new jsPDF(), model);
  assert.equal(doc.internal.getNumberOfPages(), 1);
  assert.deepEqual(model.editorial.actions, []);
  assert.equal(model.progress.score, null);
});

test('a 30-day review retains each date and keeps content above the page footer', () => {
  const payload = syntheticReport();
  Object.assign(payload.local_data, { period_start: '2026-09-01', period_end: '2026-09-30', period_days: 30,
    daily_totals: Array.from({ length: 30 }, (_, index) => ({ date: `2026-09-${String(index + 1).padStart(2, '0')}`, total_seconds: (index + 1) * 60 })) });
  const model = createStatusReportViewModel(payload), doc = new jsPDF(), drawn = [];
  const original = doc.text.bind(doc);
  doc.text = (value, x, y, ...rest) => { drawn.push({ value: String(value), y, page: doc.internal.getCurrentPageInfo().pageNumber }); return original(value, x, y, ...rest); };
  renderStatusReportPdf(doc, model);
  assert.equal(model.days.length, 30);
  assert.equal(doc.internal.getNumberOfPages(), 3);
  for (let day = 1; day <= 30; day++) assert.ok(drawn.some(row => row.page === 1 && row.value === String(day).padStart(2, '0')));
  assert.ok(drawn.every(row => row.y <= 291));
});

test('ambiguous work tools remain in local details and never become an app-closing recommendation', () => {
  const payload = syntheticReport();
  payload.local_data.distraction_app_analysis = { detours: [{ label: 'Windows Explorer', kind: 'other', seconds: 4000, visits: 20, days: 4 }] };
  const model = createStatusReportViewModel(payload), doc = new jsPDF(), drawn = [];
  const original = doc.text.bind(doc);
  doc.text = (value, ...args) => { drawn.push(String(value)); return original(value, ...args); };
  renderStatusReportPdf(doc, model);
  assert.equal(model.editorial.appContexts.length, 0);
  assert.match(model.editorial.appMessage, /need context/);
  assert.ok(!drawn.some(value => value.includes('Windows Explorer')));
  const screen = renderStatusReportHtml(model);
  assert.ok(screen.indexOf('Windows Explorer') > screen.indexOf('Read the full local analysis'));
  assert.ok(model.actions.every(action => !action.includes('close') && !action.includes('Windows Explorer')));
});
