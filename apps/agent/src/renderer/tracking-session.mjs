export const TRACKING_CHECKPOINT_KEY = 'flowsight_tracking_checkpoint_v1';
export const TRACKING_CHECKPOINT_MAX_AGE_MS = 2 * 60 * 1000;

const CHECKPOINT_VERSION = 1;
const VALID_MODES = new Set(['running', 'paused']);
const MAX_CLOCK_SKEW_MS = 30 * 1000;

function normalizeSeconds(value) {
  const seconds = Number(value);
  return Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
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
 * Reconciles durable SQLite history, the native agent state, and the renderer
 * checkpoint without ever counting time spent during a native-process outage.
 */
export function resolveTrackingRestore({
  nativeRunning,
  serverOnline,
  historySeconds,
  checkpoint,
  agentFocusStatus,
}) {
  const historyTotal = normalizeSeconds(historySeconds);
  const checkpointTotal = checkpoint ? normalizeSeconds(checkpoint.totalSeconds) : 0;

  if (nativeRunning) {
    const liveCheckpointTotal = checkpoint?.mode === 'running'
      ? checkpointTotal + normalizeSeconds(checkpoint.ageSeconds)
      : checkpointTotal;
    return {
      mode: 'running',
      totalSeconds: Math.max(historyTotal, liveCheckpointTotal),
      shouldResumeNative: false,
    };
  }

  if (agentFocusStatus === 'paused') {
    return {
      mode: 'paused',
      totalSeconds: Math.max(historyTotal, checkpointTotal),
      shouldResumeNative: false,
    };
  }

  if (checkpoint?.mode === 'running') {
    return {
      mode: serverOnline ? 'running' : 'paused',
      totalSeconds: Math.max(historyTotal, checkpointTotal),
      shouldResumeNative: Boolean(serverOnline),
    };
  }

  if (checkpoint?.mode === 'paused') {
    return {
      mode: 'paused',
      totalSeconds: Math.max(historyTotal, checkpointTotal),
      shouldResumeNative: false,
    };
  }

  return {
    mode: 'stopped',
    totalSeconds: historyTotal,
    shouldResumeNative: false,
  };
}
