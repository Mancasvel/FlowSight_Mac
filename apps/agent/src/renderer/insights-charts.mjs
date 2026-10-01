import {getLanguage} from './i18n.mjs';
const TASK_COLOR_KEYS = Object.freeze({
  analysis: 'analysis',
  coding: 'coding',
  debugging: 'debugging',
  codereview: 'review',
  testing: 'testing',
  design: 'design',
  devops: 'devops',
  database: 'database',
  research: 'research',
  documentation: 'documentation',
  planning: 'planning',
  communication: 'communication',
  meeting: 'communication',
  admin: 'other',
  browsing: 'browsing',
  idle: 'other',
  general: 'general',
});

export function taskColorKeyForCategory(category) {
  const key = String(category ?? '').toLowerCase().replace(/[^a-z]/g, '');
  return TASK_COLOR_KEYS[key] || 'other';
}

export function taskSharePercent(seconds, totalSeconds) {
  const part = Number(seconds);
  const total = Number(totalSeconds);
  if (!Number.isFinite(part) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.max(0, Math.min(100, (part / total) * 100));
}

export function buildTaskBreakdown(data) {
  const entries = data?.entries || [];
  const ticketCategories = new Map();
  const unticketed = new Map();

  for (const entry of entries) {
    const seconds = Math.max(0, Number(entry.duration_seconds) || 0);
    const category = String(entry.category || 'General').trim() || 'General';
    if (entry.ticket) {
      const categories = ticketCategories.get(entry.ticket) || new Map();
      categories.set(category, (categories.get(category) || 0) + seconds);
      ticketCategories.set(entry.ticket, categories);
    } else {
      unticketed.set(category, (unticketed.get(category) || 0) + seconds);
    }
  }

  const items = (data?.ticket_breakdown || []).map(ticket => {
    const categories = [...(ticketCategories.get(ticket.ticket) || new Map())];
    categories.sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    return {
      label: ticket.ticket,
      seconds: ticket.total_seconds,
      category: categories[0]?.[0] || 'General',
    };
  });
  for (const [label, seconds] of unticketed) {
    items.push({ label, seconds, category: label });
  }
  if (!items.length) {
    for (const category of data?.category_breakdown || []) {
      items.push({
        label: category.category,
        seconds: category.total_seconds,
        category: category.category,
      });
    }
  }
  return items.sort((a, b) => b.seconds - a.seconds || a.label.localeCompare(b.label));
}

// Entries end at their UTC timestamp. Split each interval by local minute so
// a block crossing an hour, midnight, or a DST transition lands in the right slot.
export function bucketFocusEntriesByHour(entries, localDate) {
  const byHour = new Array(24).fill(0);
  for (const entry of entries) {
    const end = new Date(entry.time).getTime();
    const duration = Math.min(86400, Math.max(0, Number(entry.duration_seconds) || 0));
    if (!Number.isFinite(end) || !duration) continue;
    let cursor = end - duration * 1000;
    while (cursor < end) {
      const next = Math.min(end, (Math.floor(cursor / 60000) + 1) * 60000);
      const local = new Date(cursor);
      const date = `${local.getFullYear()}-${String(local.getMonth() + 1).padStart(2, '0')}-${String(local.getDate()).padStart(2, '0')}`;
      if (date === localDate) byHour[local.getHours()] += (next - cursor) / 1000;
      cursor = next;
    }
  }
  return byHour;
}

export function focusChartSlots(byHour) {
  const active = byHour.map((seconds, hour) => seconds > 0 ? hour : -1).filter(hour => hour >= 0);
  if (!active.length) return [];
  let first = Math.max(0, active[0] - 1);
  let last = Math.min(23, active.at(-1) + 1);
  while (last - first + 1 < 8) {
    if (first > 0) first--;
    if (last - first + 1 < 8 && last < 23) last++;
  }
  return Array.from({ length: last - first + 1 }, (_, index) => ({
    hour: first + index,
    seconds: byHour[first + index] || 0,
  }));
}

export function formatChartHour(hour) {
  return getLanguage()==='es' ? `${String(hour).padStart(2,'0')}:00` : `${hour % 12 || 12}${hour < 12 ? 'am' : 'pm'}`;
}

// Each column is one clock hour, so a full column always means 60 minutes.
export function focusBarPercent(seconds) {
  const value = Number(seconds);
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.min(100, Math.round(value / 3600 * 100));
}
