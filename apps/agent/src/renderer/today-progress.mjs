/** Wall-clock progress through the scheduled calendar event, independent of tracking. */
export function calendarEventElapsedProgress(event, nowMs = Date.now()) {
  const startMs = Date.parse(event?.startAt ?? '');
  const endMs = Date.parse(event?.endAt ?? '');
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || !Number.isFinite(nowMs) || endMs <= startMs) {
    return null;
  }

  const durationMs = endMs - startMs;
  const elapsedMs = Math.min(Math.max(nowMs - startMs, 0), durationMs);
  const fraction = elapsedMs / durationMs;
  return {
    elapsedSeconds: Math.floor(elapsedMs / 1000),
    totalSeconds: Math.ceil(durationMs / 1000),
    fraction,
    percent: Math.round(fraction * 100),
  };
}

export function formatCompactDuration(seconds) {
  const safeSeconds = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}
