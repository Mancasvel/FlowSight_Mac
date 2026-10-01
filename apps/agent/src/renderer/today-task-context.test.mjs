import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolveTaskContext, transitionTaskDetail } from './today-task-context.mjs';

const markup = readFileSync(new URL('./index.html', import.meta.url), 'utf8');

test('the live calendar event is primary; manual detail and issue remain secondary', () => {
  assert.deepEqual(resolveTaskContext({
    calendarEvent: { title: 'Design review' },
    canIntegrate: true,
    selectedValue: 'PROJ-12',
    selectedLabel: '[PROJ-12] Finish mockups',
    manualTask: '  Review the onboarding flow  ',
  }), {
    task: 'Design review — Review the onboarding flow',
    jiraTicket: 'PROJ-12',
  });
});

test('a calendar event works without a linked issue or extra description', () => {
  assert.deepEqual(resolveTaskContext({ calendarEvent: { title: 'Client call' }, canIntegrate: true }), {
    task: 'Client call', jiraTicket: null,
  });
});

test('without one calendar event, linked and manually entered tasks remain available', () => {
  assert.deepEqual(resolveTaskContext({
    calendarEvent: null, canIntegrate: true, selectedValue: 'PROJ-12', selectedLabel: '[PROJ-12] Finish mockups',
  }), { task: '[PROJ-12] Finish mockups', jiraTicket: 'PROJ-12' });
  assert.deepEqual(resolveTaskContext({
    calendarEvent: null, canIntegrate: true, selectedValue: 'MANUAL', manualTask: 'Write release notes',
  }), { task: 'Write release notes', jiraTicket: null });
  assert.deepEqual(resolveTaskContext({
    calendarEvent: null, canIntegrate: false, manualTask: 'Write release notes',
  }), { task: 'Write release notes', jiraTicket: null });
  assert.deepEqual(resolveTaskContext({
    calendarEvent: null, canIntegrate: true, selectedValue: 'PROJ-12',
    selectedLabel: '[PROJ-12] Finish mockups', manualTask: 'Write release notes',
  }), { task: 'Write release notes', jiraTicket: 'PROJ-12' });
});

test('the event is inside the main timer, with editable context before daily-goal settings', () => {
  const timer = markup.indexOf('class="today-timer-ring"');
  const event = markup.indexOf('id="todayCalendarContext"');
  const progress = markup.indexOf('id="todayCalendarProgressTrack"');
  const detail = markup.indexOf('id="todayManualTaskWrap"');
  const linked = markup.indexOf('id="todayIntegrationControls"');
  const goal = markup.indexOf('id="dailyGoalSelect"');
  assert.ok(timer < event && event < progress && progress < detail && detail < linked && linked < goal);
  assert.doesNotMatch(markup, /id="calendarCurrent"/);
});

test('Study is a local task with or without integrations and never becomes a Jira ticket', () => {
  for (const canIntegrate of [false, true]) {
    assert.deepEqual(resolveTaskContext({ canIntegrate, selectedValue: 'STUDY', selectedLabel: 'Study' }), {
      task: 'Study', jiraTicket: null,
    });
    assert.deepEqual(resolveTaskContext({ canIntegrate, selectedValue: 'STUDY', selectedLabel: 'Estudio' }), {
      task: 'Estudio', jiraTicket: null,
    });
    assert.deepEqual(resolveTaskContext({ canIntegrate, selectedValue: 'STUDY', manualTask: 'Four ADDA exercises' }), {
      task: 'Four ADDA exercises', jiraTicket: null,
    });
    assert.deepEqual(resolveTaskContext({
      calendarEvent: { title: 'ADDA exercises' }, canIntegrate, selectedValue: 'STUDY', selectedLabel: 'Study',
    }), { task: 'ADDA exercises', jiraTicket: null });
  }
});

test('manual detail stays with its calendar event and does not leak between accounts', () => {
  const cache = new Map();
  assert.equal(transitionTaskDetail(cache, null, 'owner-a:manual', ''), '');
  assert.equal(transitionTaskDetail(cache, 'owner-a:manual', 'owner-a:google:event', 'Manual task'), '');
  assert.equal(transitionTaskDetail(cache, 'owner-a:google:event', 'owner-b:manual', 'Meeting detail'), '');
  assert.equal(transitionTaskDetail(cache, 'owner-b:manual', 'owner-a:google:event', 'Other user task'), 'Meeting detail');
  assert.equal(transitionTaskDetail(cache, 'owner-a:google:event', 'owner-a:manual', 'Updated meeting detail'), 'Manual task');
});
