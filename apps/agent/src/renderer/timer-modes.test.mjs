import test from 'node:test';
import assert from 'node:assert/strict';
import { PomodoroTimer, loadTimerPreferences, saveTimerPreferences } from './timer-modes.mjs';
import { LiveTrackingClock } from './tracking-session.mjs';

test('normal counter ticks from the native sample and analysis never adds time twice', () => {
  const wall = new Date(2026, 9, 1, 12).getTime();
  const clock = new LiveTrackingClock();
  clock.sample({ date: '2026-10-01', total_seconds: 20700, is_running: true }, 100, wall);
  assert.equal(clock.seconds(1100, wall + 1000), 20701);
  assert.equal(clock.seconds(2100, wall + 2000), 20702);
  clock.seed({ date: '2026-10-01', total_seconds: 21571 }, 2100, wall + 2000);
  assert.equal(clock.seconds(2100, wall + 2000), 20702);
  clock.sample({ date: '2026-10-01', total_seconds: 20702, is_running: true }, 2100, wall + 2000);
  assert.equal(clock.seconds(3100, wall + 3000), 20703);
  clock.setRunning(false, 3100, wall + 3000);
  assert.equal(clock.seconds(63100, wall + 63000), 20703);
  clock.setRunning(true, 63100, wall + 63000);
  assert.equal(clock.seconds(64100, wall + 64000), 20704);
});

test('normal timer counts delayed callbacks correctly and resets at local midnight', () => {
  const wall = new Date(2026, 9, 1, 23, 59, 58).getTime();
  const clock = new LiveTrackingClock();
  clock.sample({ date: '2026-10-01', total_seconds: 10, is_running: true }, 0, wall);
  assert.equal(clock.seconds(5000, wall + 5000), 3);
  assert.equal(clock.seconds(7000, wall + 7000), 5);
});

test('Pomodoro pauses work, completes once and takes a long break after four intervals', () => {
  const timer = new PomodoroTimer({ work: 25, shortBreak: 5, longBreak: 15 });
  let tick = 0;
  for (let interval = 1; interval <= 4; interval++) {
    timer.start(tick);
    assert.equal(timer.remaining(tick + 1000), 1499000);
    timer.pause(tick + 1000);
    assert.equal(timer.remaining(tick + 61000), 1499000);
    timer.start(tick + 61000);
    assert.equal(timer.update(tick + 1560000), 'work-complete');
    assert.equal(timer.update(tick + 1560001), null);
    timer.finishWork();
    assert.equal(timer.phase, interval === 4 ? 'long-break' : 'break');
    assert.equal(timer.remainingMs, (interval === 4 ? 15 : 5) * 60000);
    timer.start(tick + 1560000);
    tick += 1560000 + timer.remainingMs;
    assert.equal(timer.update(tick), 'break-complete');
    assert.equal(timer.ready, true);
    assert.equal(timer.anchor, null, 'Tracking must wait for an explicit next interval.');
  }
});

test('mode and intervals persist without requiring browser storage', () => {
  const values = new Map();
  const storage = { getItem: key => values.get(key), setItem: (key, value) => values.set(key, value) };
  assert.equal(loadTimerPreferences(storage).mode, 'normal');
  saveTimerPreferences(storage, { mode: 'pomodoro', work: 50, shortBreak: 10, longBreak: 20 });
  assert.deepEqual(loadTimerPreferences(storage), { mode: 'pomodoro', work: 50, shortBreak: 10, longBreak: 20 });
  assert.equal(loadTimerPreferences({ getItem() { throw new Error('unavailable'); } }).mode, 'normal');
});
