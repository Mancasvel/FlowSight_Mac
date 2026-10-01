import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, getLocale, getLanguage, initializeLocalization, localizeStatus } from './i18n.mjs';
export function sessionWindow(startTime, endTime, now = new Date()) {
  const makeDate = (value) => {
    if (!/^\d{2}:\d{2}$/.test(value)) throw new Error(tr('Choose valid start and end times.'));
    const [h, m] = value.split(':').map(Number);
    if (h > 23 || m > 59) throw new Error(tr('Choose valid times.'));
    const date = new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, m);
    if (date.getHours() !== h || date.getMinutes() !== m) throw new Error(tr('That time is unavailable because the clocks change.'));
    return date;
  };
  const start = makeDate(startTime), end = makeDate(endTime);
  if (start < now) throw new Error(tr('Choose a start later today.'));
  const minutes = (end - start) / 60000;
  if (minutes < 15 || minutes > 960) throw new Error(tr('Choose 15 minutes to 16 hours, ending today.'));
  const localTimestamp = (date) => {
    const pad = (n) => String(n).padStart(2, '0');
    const offset = -date.getTimezoneOffset();
    const zone = `${offset >= 0 ? '+' : '-'}${pad(Math.floor(Math.abs(offset) / 60))}:${pad(Math.abs(offset) % 60)}`;
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:00${zone}`;
  };
  return { startAt: localTimestamp(start), endAt: localTimestamp(end) };
}

const clock = (value) => new Date(value).toLocaleTimeString(getLocale(), { hour: '2-digit', minute: '2-digit' });
const get = (id) => document.getElementById(id);

export const calendarDestinationLabel = destination => destination?.provider === 'google' ? 'Google Calendar' : destination?.provider === 'microsoft' ? 'Microsoft Calendar' : 'FlowSight';
export function sessionConfirmationText(result) {
  const count = result.events.length, calendar = calendarDestinationLabel(result.calendarDestination);
  return tr(count === 1 ? '{count} block added to {calendar}.' : '{count} blocks added to {calendar}.', {count, calendar});
}

function renderBlocks(host, blocks) {
  host.replaceChildren();
  for (const block of blocks) {
    const row = document.createElement('li'), time = document.createElement('span'), content = document.createElement('div');
    time.className = 'session-block-time';
    setText(time, () => (`${clock(block.startAt)}–${clock(block.endAt)}`));
    const title = document.createElement('strong'); setText(title, () => block.localizedTitle?.[getLanguage()] || block.title);
    content.append(title);
    if (block.rationale) { const p = document.createElement('p'); setText(p, () => block.localizedRationale?.[getLanguage()] || block.rationale); content.append(p); }
    row.append(time, content); host.append(row);
  }
}

export function mountSessionPlanner({ invoke }) {
  let proposal = null, recoverySave = null, busy = false, expiryTimer;
  const panel = get('sessionPlanner'), form = get('sessionPlanForm'), status = get('sessionPlanStatus');
  const intention = get('sessionIntention'), start = get('sessionStart'), end = get('sessionEnd'), feedback = get('sessionFeedback');
  function message(text, error = false) { setText(status, () => typeof text==='function'?text():localizeStatus(text)); status.dataset.state = error ? 'error' : ''; }
  function lock(value) {
    busy = value;
    for (const id of ['sessionIntention', 'sessionStart', 'sessionEnd', 'sessionFeedback', 'sessionGenerate', 'sessionRevise', 'sessionConfirm', 'sessionCancel']) get(id).disabled = value || Boolean(recoverySave);
    get('sessionConfirm').disabled = value || !proposal;
    get('sessionRetrySave').disabled = value || !recoverySave;
    get('sessionAbandonSave').disabled = value || !recoverySave;
    setText(get('sessionGenerate'), () => ((value ? tr('Planning on this device…') : tr('Suggest my session'))));
    form.setAttribute('aria-busy', String(value));
  }
  async function discard() {
    clearTimeout(expiryTimer);
    const old = proposal; proposal = null;
    get('sessionProposal').hidden = true; get('sessionConfirm').disabled = true;
    if (old) await invoke('cancel_session_plan', { id: old.id });
  }
  async function plan(revise) {
    if (busy || recoverySave) return;
    if (revise && (!proposal || !feedback.value.trim())) { message('Describe what you want to change.', true); feedback.focus(); return; }
    try {
      const request = { intention: intention.value.trim(), ...sessionWindow(start.value, end.value) };
      if (!request.intention) throw new Error(tr('Describe what you want to work on today.'));
      if (!revise) await discard();
      lock(true); message('The local agent is considering your available hours, tasks and saved preferences…');
      proposal = await invoke('propose_session_plan', { request, previousId: revise ? proposal.id : null, feedback: revise ? feedback.value.trim() : null });
      const currentProposal = proposal;
      get('sessionProposal').hidden = false; setText(get('sessionPlanSummary'), () => currentProposal.localizedSummary?.[getLanguage()] || currentProposal.summary);
      renderBlocks(get('sessionPlanBlocks'), proposal.blocks);
      const overflow = get('sessionUnscheduled'); overflow.replaceChildren(); overflow.hidden = !proposal.unscheduled.length;
      if (proposal.unscheduled.length) {
        const heading = document.createElement('strong'); setText(heading, () => (tr('Work that needs more time'))); overflow.append(heading);
        proposal.unscheduled.forEach((text,index) => { const p = document.createElement('p'); setText(p, () => currentProposal.localizedUnscheduled?.[getLanguage()]?.[index] || text); overflow.append(p); });
      }
      feedback.value = ''; message(() => tr('Draft ready. Review the times and estimates before adding the blocks to {calendar}.', {calendar:calendarDestinationLabel(currentProposal.calendarDestination)}));
      clearTimeout(expiryTimer);
      expiryTimer = setTimeout(() => { proposal = null; get('sessionConfirm').disabled = true; message('This draft expired. Suggest a fresh session.', true); }, proposal.expiresInSeconds * 1000);
    } catch (error) { message(()=>formatMessage`${localizeStatus(String(error))} Edit the request and try again.`, true); }
    finally { lock(false); }
  }
  form.addEventListener('submit', (event) => { event.preventDefault(); plan(false); });
  get('sessionRevise').addEventListener('click', () => plan(true));
  for (const field of [intention, start, end]) field.addEventListener('input', () => {
    if (proposal) { discard().catch((error) => message(String(error), true)); message('Session details changed. Suggest a fresh plan before confirming.'); }
  });
  get('sessionCancel').addEventListener('click', async () => {
    if (busy) return;
    try { await discard(); message('Draft discarded. Edit your request to make another plan.'); }
    catch (error) { message(String(error), true); }
  });
  async function confirm(id) {
    if (busy || !id) return;
    lock(true);
    message('Saving the reviewed blocks to your calendar…');
    try {
      const result = await invoke('confirm_session_plan', { id });
      proposal = null; clearTimeout(expiryTimer); get('sessionProposal').hidden = true;
      message(() => sessionConfirmationText(result)); await refreshCalendar();
    } catch (error) { message(String(error), true); await refreshCalendar(); }
    finally { lock(false); }
  }
  get('sessionConfirm').addEventListener('click', () => confirm(proposal?.id));
  get('sessionRetrySave').addEventListener('click', () => confirm(recoverySave?.id));
  get('sessionAbandonSave').addEventListener('click', async () => {
    if (busy || !recoverySave) return;
    if (!window.confirm(tr('Stop saving the remaining blocks? Events already sent will stay in your linked calendar. An interrupted request may also have created an event there. Check your calendar before planning again.'))) return;
    const id = recoverySave.id;
    lock(true);
    try { await invoke('abandon_session_plan', {id}); await refreshCalendar(); message('Remaining save stopped. Existing linked-calendar events have been kept.'); }
    catch (error) { message(String(error),true); }
    finally {lock(false);}
  });
  function open() {
    panel.open = true;
    if (!start.value) {
      const later = new Date(Math.ceil((Date.now() + 5 * 60000) / 300000) * 300000);
      if (later.toDateString() === new Date().toDateString()) {
        const asTime = (d) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
        start.value = asTime(later); const finish = new Date(later.getTime() + 7200000);
        end.value = finish.toDateString() === later.toDateString() ? asTime(finish) : '23:59';
      }
    }
    intention.focus(); panel.scrollIntoView({ block: 'start' });
  }
  get('sessionPlannerToggle').addEventListener('click', () => { if (!panel.open) setTimeout(open, 0); });
  async function refreshCalendar() {
    try {
      const data = await invoke('get_local_agent_data'), now = new Date();
      recoverySave = (data.sessionSaves || []).find(save => !save.complete && !save.abandoned) || null;
      get('sessionRecovery').hidden = !recoverySave;
      if (recoverySave) {
        proposal = null; clearTimeout(expiryTimer); get('sessionProposal').hidden = true;
        const save = recoverySave;
        setText(get('sessionRecoveryStatus'), () => tr('The reviewed session is not fully saved to {calendar}: {saved} of {total} blocks confirmed. Retry to finish saving these same blocks.', {calendar:calendarDestinationLabel(save.target),saved:save.events.filter(event => event.externalId).length,total:save.events.length}));
        renderBlocks(get('sessionRecoveryBlocks'), save.events);
      }
      lock(busy);
      const events = (data.events || []).filter((e) => new Date(e.startAt).toDateString() === now.toDateString() && new Date(e.endAt) > now)
        .sort((a, b) => Date.parse(a.startAt) - Date.parse(b.startAt));
      get('sessionCalendar').hidden = !events.length; renderBlocks(get('sessionCalendarBlocks'), events);
    } catch (_) { /* Refresh again when the agent becomes available. */ }
  }
  return { open, refreshCalendar };
}
