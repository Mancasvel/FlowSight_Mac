import test from 'node:test';
import assert from 'node:assert/strict';

import {
  TRACKING_CHECKPOINT_KEY,
  clearTrackingCheckpoint,
  createTrackingCheckpoint,
  loadTrackingCheckpoint,
  resolveTrackingRestore,
  recordedDailyTotal,
  saveTrackingCheckpoint,
} from './tracking-session.mjs';

function memoryStorage() {
  const entries = new Map();
  return {
    getItem: (key) => entries.get(key) ?? null,
    setItem: (key, value) => entries.set(key, value),
    removeItem: (key) => entries.delete(key),
  };
}

test('round-trips a fresh checkpoint and reports its age', () => {
  const storage = memoryStorage();
  const now = new Date(2026, 7, 23, 10, 0, 0).getTime();

  saveTrackingCheckpoint(storage, 'running', 123.9, now);
  assert.deepEqual(loadTrackingCheckpoint(storage, { now: now + 5_500 }), {
    ...createTrackingCheckpoint('running', 123, now),
    ageSeconds: 5,
  });
});

test('discards stale, cross-day, and malformed checkpoints', () => {
  const storage = memoryStorage();
  const now = new Date(2026, 7, 23, 23, 59, 59).getTime();
  saveTrackingCheckpoint(storage, 'running', 20, now);

  assert.equal(loadTrackingCheckpoint(storage, { now: now + 2_000 }), null);
  assert.equal(storage.getItem(TRACKING_CHECKPOINT_KEY), null);

  saveTrackingCheckpoint(storage, 'paused', 20, now);
  assert.equal(loadTrackingCheckpoint(storage, { now: now + 1_500, maxAgeMs: 1_000 }), null);
  assert.equal(storage.getItem(TRACKING_CHECKPOINT_KEY), null);

  storage.setItem(TRACKING_CHECKPOINT_KEY, '{broken');
  assert.equal(loadTrackingCheckpoint(storage, { now }), null);
  assert.equal(storage.getItem(TRACKING_CHECKPOINT_KEY), null);
});

test('restores tracking intent while taking displayed time from recorded history', () => {
  const checkpoint = {
    ...createTrackingCheckpoint('running', 100, Date.now()),
    ageSeconds: 12,
  };

  assert.deepEqual(resolveTrackingRestore({
    nativeRunning: true,
    serverOnline: true,
    historySeconds: 80,
    checkpoint,
  }), {
    mode: 'running',
    totalSeconds: 80,
    shouldResumeNative: false,
  });

  assert.deepEqual(resolveTrackingRestore({
    nativeRunning: false,
    serverOnline: true,
    historySeconds: 80,
    checkpoint,
  }), {
    mode: 'running',
    totalSeconds: 80,
    shouldResumeNative: true,
  });
});

test('the reported 05:59:31 checkpoint cannot inflate 5 h 45 min of recorded time', () => {
  const checkpoint = { ...createTrackingCheckpoint('running', 21571), ageSeconds: 15 };
  for (const nativeRunning of [false, true]) {
    assert.equal(resolveTrackingRestore({ nativeRunning, serverOnline: true, historySeconds: 20700, checkpoint }).totalSeconds, 20700);
  }
  assert.equal(resolveTrackingRestore({ nativeRunning: false, serverOnline: false, historySeconds: 20700, checkpoint }).totalSeconds, 20700);
});

test('daily totals use the current local date and accept durable corrections', () => {
  const now = new Date(2026, 9, 1, 12).getTime();
  assert.equal(recordedDailyTotal({ date: '2026-10-01', total_seconds: 20700 }, now), 20700);
  assert.equal(recordedDailyTotal({ date: '2026-09-30', total_seconds: 21571 }, now), 0);
  assert.equal(recordedDailyTotal({ date: '2026-10-01', total_seconds: -1 }, now), 0);
  assert.equal(recordedDailyTotal({ date: '2026-10-01', total_seconds: NaN }, now), 0);
});

test('preserves intent as paused when the local server did not survive a restart', () => {
  const checkpoint = createTrackingCheckpoint('running', 75, Date.now());
  assert.deepEqual(resolveTrackingRestore({
    nativeRunning: false,
    serverOnline: false,
    historySeconds: 90,
    checkpoint,
  }), {
    mode: 'paused',
    totalSeconds: 90,
    shouldResumeNative: false,
  });
});

test('an agent focus block stays paused even when the local server is online', () => {
  const checkpoint = createTrackingCheckpoint('running', 75, Date.now());
  assert.deepEqual(resolveTrackingRestore({
    nativeRunning: false,
    serverOnline: true,
    historySeconds: 90,
    checkpoint,
    agentFocusStatus: 'paused',
  }), {
    mode: 'paused',
    totalSeconds: 90,
    shouldResumeNative: false,
  });
});

test('uses durable history for a clean stop and can clear the checkpoint', () => {
  const storage = memoryStorage();
  saveTrackingCheckpoint(storage, 'paused', 42);
  clearTrackingCheckpoint(storage);
  assert.equal(storage.getItem(TRACKING_CHECKPOINT_KEY), null);

  assert.deepEqual(resolveTrackingRestore({
    nativeRunning: false,
    serverOnline: false,
    historySeconds: 61,
    checkpoint: null,
  }), {
    mode: 'stopped',
    totalSeconds: 61,
    shouldResumeNative: false,
  });
});
