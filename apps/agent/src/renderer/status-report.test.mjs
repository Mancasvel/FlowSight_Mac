import { setLanguagePreference } from './i18n.mjs';
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
    assert.ok(drawn.some(line=>line.includes('Revisa el siguiente bloque.')));assert.ok(drawn.some(line=>line.includes('CONCENTRACIÓN SOSTENIDA')));assert.ok(drawn.some(line=>line.includes('Review · María')));
  } finally {setLanguagePreference('en',{persist:false});}
});

// Synthetic activity only: these numbers are fixtures, not a customer report.
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

test('actions lead the evidence and untrusted report text is escaped', () => {
  const payload = syntheticReport();
  payload.report.recommendations[0] = 'Review <script>alert(1)</script> breaks.';
  const html = renderStatusReportHtml(createStatusReportViewModel(payload));
  assert.ok(html.indexOf('What to do next') < html.indexOf('The week in view'));
  assert.ok(html.includes('&lt;script&gt;'));
  assert.ok(!html.includes('<script>'));
  assert.ok(html.includes('2026-09-22: 0.0 hours'));
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

test('missing AI lessons recover from recorded evidence in the review and PDF', () => {
  const payload = syntheticReport();
  payload.report.lessons_learned = [{ title: '', body: 'Incomplete AI item' }];
  const model = createStatusReportViewModel(payload);

  assert.ok(model.lessons.some((lesson) => lesson.body.includes('6.0h (60% of tracked time)')));
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
  assert.ok(drawn.some((line) => line.includes('The work mix had a clear centre')));
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
  assert.ok(html.indexOf('Attention detours') < html.indexOf('The week in view'));
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
  assert.ok(drawn.some((line) => line.includes('Attention detours')));
  assert.ok(drawn.some((line) => line.includes('Apple <Music>')));
  assert.ok(drawn.some((line) => line.includes('Next session:')));
  assert.ok(doc.internal.getNumberOfPages() <= 2);
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

test('PDF keeps each app section heading with its first app across page boundaries', () => {
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
      if (line === 'Attention detours' || line === 'Sample service') {
        pages[line] = doc.internal.getCurrentPageInfo().pageNumber;
      }
      return originalText(value, ...args);
    };
    renderStatusReportPdf(doc, createStatusReportViewModel(payload));
    assert.equal(pages['Attention detours'], pages['Sample service']);
  }
});

test('PDF keeps the distraction methodology note together at a page boundary', () => {
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
  const caveatPages = new Set();
  const originalText = doc.text.bind(doc);
  doc.text = (value, ...args) => {
    if (String(value).includes('APP_CAVEAT')) {
      caveatPages.add(doc.internal.getCurrentPageInfo().pageNumber);
    }
    return originalText(value, ...args);
  };
  renderStatusReportPdf(doc, model);
  assert.equal(caveatPages.size, 1);
});

test('a concise review keeps its charts and findings within two PDF pages', () => {
  const model = createStatusReportViewModel(syntheticReport());
  const doc = renderStatusReportPdf(new jsPDF(), model);
  assert.ok(doc.internal.getNumberOfPages() <= 2);
});

test('PDF paginates long report copy without drawing text below the footer', () => {
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
  assert.ok(doc.internal.getNumberOfPages() >= 3);
  assert.ok(doc.output('arraybuffer').byteLength > 20_000);
  assert.ok(drawn.some((entry) => entry.value.includes('END_ACTION_5')));
  assert.ok(drawn.some((entry) => entry.value.includes('END_AREA_6')));
});
