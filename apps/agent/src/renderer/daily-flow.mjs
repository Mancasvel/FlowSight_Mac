import { localDayKey } from './tracking-session.mjs';

export const DAILY_WIN_SECONDS = 15 * 60;
export const WEEKLY_WIN_TARGET = 3;
export const MILESTONES = [
  { days: 1, name: 'First step' },
  { days: 3, name: 'Finding rhythm' },
  { days: 7, name: 'Building momentum' },
  { days: 30, name: 'Steady practice' },
];

function dateFromKey(key) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(key ?? '')) return null;
  const [year, month, day] = key.split('-').map(Number);
  const date = new Date(year, month - 1, day, 12);
  return localDayKey(date) === key ? date : null;
}

function previousDay(key) {
  const date = dateFromKey(key);
  date.setDate(date.getDate() - 1);
  return localDayKey(date);
}

/** Local calendar arithmetic survives DST. A pending today preserves yesterday's streak. */
export function dailyFlowView(progress, now = Date.now()) {
  const today = localDayKey(now);
  const completed = new Set((Array.isArray(progress?.completed_dates) ? progress.completed_dates : [])
    .filter(key => dateFromKey(key) && key <= today));
  const rawSeconds = progress?.date === today ? Number(progress.total_seconds) : 0;
  const seconds = Number.isFinite(rawSeconds) ? Math.max(0, Math.floor(rawSeconds)) : 0;
  const todayDone = completed.has(today);
  let cursor = todayDone ? today : previousDay(today), streak = 0;
  while (completed.has(cursor)) { streak++; cursor = previousDay(cursor); }
  if (Number.isSafeInteger(progress?.streak) && progress.streak >= 0) {
    streak = progress.date === today ? progress.streak : 0;
  }
  const totalWins = Number.isSafeInteger(progress?.total_wins) && progress.total_wins >= 0
    ? progress.total_wins : completed.size;
  const monday = dateFromKey(today);
  monday.setDate(monday.getDate() - (monday.getDay() + 6) % 7);
  const week = Array.from({ length: 7 }, (_, offset) => {
    const date = new Date(monday);
    date.setDate(date.getDate() + offset);
    const key = localDayKey(date);
    return { key, date, done: completed.has(key), today: key === today, future: key > today };
  });
  return {
    today, todayDone, seconds, streak, totalWins, week,
    weekWins: week.filter(day => day.done).length,
    remainingMinutes: Math.max(0, Math.ceil((DAILY_WIN_SECONDS - seconds) / 60)),
    percent: todayDone ? 100 : Math.min(100, Math.floor(seconds / DAILY_WIN_SECONDS * 100)),
    milestone: MILESTONES.find(item => item.days > totalWins) ?? null,
  };
}
