function localDateFromIso(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(value || ''));
  if (!match) return null;
  const date = new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
  return date.getFullYear() === Number(match[1])
    && date.getMonth() === Number(match[2]) - 1
    && date.getDate() === Number(match[3]) ? date : null;
}

export function isWeeklyReportDue(schedule, now = new Date()) {
  if (!schedule?.enabled || !Number.isInteger(schedule.weekday)
      || schedule.weekday < 0 || schedule.weekday > 6
      || schedule.weekday !== now.getDay()) return false;

  const match = /^([01]\d|2[0-3]):([0-5]\d)$/.exec(String(schedule.time || ''));
  if (!match) return false;
  const scheduledMinutes = Number(match[1]) * 60 + Number(match[2]);
  if (now.getHours() * 60 + now.getMinutes() < scheduledMinutes) return false;

  const lastDate = localDateFromIso(schedule.lastGeneratedDate);
  if (!lastDate) return true;
  const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - ((now.getDay() + 6) % 7));
  const nextMonday = new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + 7);
  return lastDate < monday || lastDate >= nextMonday;
}
