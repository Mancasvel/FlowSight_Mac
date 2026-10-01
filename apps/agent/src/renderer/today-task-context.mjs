export function resolveTaskContext({ calendarEvent, canIntegrate, selectedValue, selectedLabel, manualTask }) {
  const detail = String(manualTask || '').trim();
  const linked = Boolean(canIntegrate && selectedValue && selectedValue !== 'MANUAL' && selectedValue !== 'General');
  const jiraTicket = linked ? String(selectedValue) : null;
  const eventTitle = String(calendarEvent?.title || '').trim();

  // A unique live calendar event is the task. Manual text adds detail; a linked
  // issue remains metadata rather than replacing the event's name.
  if (eventTitle) {
    return { task: detail ? `${eventTitle} — ${detail}` : eventTitle, jiraTicket };
  }
  if (detail) {
    return { task: detail, jiraTicket };
  }
  if (linked) {
    return { task: String(selectedLabel || selectedValue).trim(), jiraTicket };
  }
  return { task: 'General', jiraTicket: null };
}

export function transitionTaskDetail(cache, previousKey, nextKey, visibleText) {
  if (previousKey === nextKey) return String(visibleText || '');
  if (previousKey !== null) cache.set(previousKey, String(visibleText || ''));
  return cache.get(nextKey) || '';
}
