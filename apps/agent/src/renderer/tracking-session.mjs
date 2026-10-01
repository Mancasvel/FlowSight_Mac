export const TRACKING_CHECKPOINT_KEY = 'flowsight_tracking_checkpoint_v1';
export const TRACKING_CHECKPOINT_MAX_AGE_MS = 2 * 60 * 1000;

const CHECKPOINT_VERSION = 1;
const VALID_MODES = new Set(['running', 'paused']);
const MAX_CLOCK_SKEW_MS = 30 * 1000;

function normalizeSeconds(value) {
  const seconds = Number(value);
  return Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
}

export function recordedDailyTotal(history, now = Date.now()) {
  return history?.date === localDayKey(now) ? normalizeSeconds(history.total_seconds) : 0;
}

/** Interpolate native monotonic samples locally; analysis totals never add time. */
export class LiveTrackingClock {
  constructor() {
    this.milliseconds = 0;
    this.anchor = null;
    this.day = localDayKey();
    this.native = false;
  }

  sample(snapshot, tick = performance.now(), wall = Date.now()) {
    if (!snapshot || snapshot.date !== localDayKey(wall)) return false;
    this.milliseconds = Math.max(0, Number(snapshot.total_milliseconds ?? snapshot.total_seconds * 1000) || 0);
    this.day = snapshot.date;
    this.anchor = snapshot.is_running ? tick : null;
    this.native = true;
    return true;
  }

  seed(history, tick = performance.now(), wall = Date.now()) {
    if (this.native) return;
    this.milliseconds = recordedDailyTotal(history, wall) * 1000;
    this.day = localDayKey(wall);
    this.anchor = null;
  }

  setRunning(running, tick = performance.now(), wall = Date.now()) {
    this.milliseconds = this.value(tick, wall);
    this.anchor = running ? tick : null;
  }

  value(tick = performance.now(), wall = Date.now()) {
    if (this.day !== localDayKey(wall)) {
      const date = new Date(wall);
      const sinceMidnight = wall - new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
      this.milliseconds = this.anchor === null ? 0 : Math.min(Math.max(0, tick - this.anchor), sinceMidnight);
      this.day = localDayKey(wall);
      this.anchor = this.anchor === null ? null : tick;
    }
    return this.milliseconds + (this.anchor === null ? 0 : Math.max(0, tick - this.anchor));
  }

  seconds(tick = performance.now(), wall = Date.now()) {
    return Math.floor(this.value(tick, wall) / 1000);
  }
}

export function localDayKey(now = Date.now()) {
  const date = new Date(now);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

export function createTrackingCheckpoint(mode, totalSeconds, now = Date.now()) {
  if (!VALID_MODES.has(mode)) {
    throw new TypeError(`Unsupported tracking mode: ${mode}`);
  }

  return {
    version: CHECKPOINT_VERSION,
    mode,
    totalSeconds: normalizeSeconds(totalSeconds),
    updatedAt: now,
    day: localDayKey(now),
  };
}

export function saveTrackingCheckpoint(storage, mode, totalSeconds, now = Date.now()) {
  try {
    const checkpoint = createTrackingCheckpoint(mode, totalSeconds, now);
    storage.setItem(TRACKING_CHECKPOINT_KEY, JSON.stringify(checkpoint));
    return checkpoint;
  } catch {
    return null;
  }
}

export function clearTrackingCheckpoint(storage) {
  try {
    storage.removeItem(TRACKING_CHECKPOINT_KEY);
  } catch {
    // Storage can be unavailable in hardened/private webviews. Tracking still works in memory.
  }
}

export function loadTrackingCheckpoint(
  storage,
  { now = Date.now(), maxAgeMs = TRACKING_CHECKPOINT_MAX_AGE_MS } = {},
) {
  try {
    const raw = storage.getItem(TRACKING_CHECKPOINT_KEY);
    if (!raw) return null;

    const checkpoint = JSON.parse(raw);
    const ageMs = now - Number(checkpoint.updatedAt);
    const isValid = checkpoint.version === CHECKPOINT_VERSION
      && VALID_MODES.has(checkpoint.mode)
      && Number.isFinite(Number(checkpoint.totalSeconds))
      && Number.isFinite(ageMs)
      && ageMs >= -MAX_CLOCK_SKEW_MS
      && ageMs <= maxAgeMs
      && checkpoint.day === localDayKey(now);

    if (!isValid) {
      clearTrackingCheckpoint(storage);
      return null;
    }

    return {
      ...checkpoint,
      totalSeconds: normalizeSeconds(checkpoint.totalSeconds),
      ageSeconds: Math.max(0, Math.floor(ageMs / 1000)),
    };
  } catch {
    clearTrackingCheckpoint(storage);
    return null;
  }
}

/**
 * Restore tracking intent from the checkpoint. Displayed time comes exclusively
 * from durable SQLite history; older wall-clock checkpoints must not inflate it.
 */
export function resolveTrackingRestore({
  nativeRunning,
  serverOnline,
  historySeconds,
  checkpoint,
  agentFocusStatus,
}) {
  const historyTotal = normalizeSeconds(historySeconds);

  if (nativeRunning) {
    return {
      mode: 'running',
      totalSeconds: historyTotal,
      shouldResumeNative: false,
    };
  }

  if (agentFocusStatus === 'paused') {
    return {
      mode: 'paused',
      totalSeconds: historyTotal,
      shouldResumeNative: false,
    };
  }

  if (checkpoint?.mode === 'running') {
    return {
      mode: serverOnline ? 'running' : 'paused',
      totalSeconds: historyTotal,
      shouldResumeNative: Boolean(serverOnline),
    };
  }

  if (checkpoint?.mode === 'paused') {
    return {
      mode: 'paused',
      totalSeconds: historyTotal,
      shouldResumeNative: false,
    };
  }

  return {
    mode: 'stopped',
    totalSeconds: historyTotal,
    shouldResumeNative: false,
  };
}
