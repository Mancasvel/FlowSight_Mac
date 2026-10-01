export function sessionWindow(startTime, endTime, now = new Date()) {
  const makeDate = (value) => {
    if (!/^\d{2}:\d{2}$/.test(value)) throw new Error('Choose valid start and end times.');
    const [h, m] = value.split(':').map(Number);
    if (h > 23 || m > 59) throw new Error('Choose valid times.');
    const date = new Date(now.getFullYear(), now.getMonth(), now.getDate(), h, m);
    if (date.getHours() !== h || date.getMinutes() !== m) throw new Error('That time is unavailable because the clocks change.');
    return date;
  };
  const start = makeDate(startTime), end = makeDate(endTime);
  if (start < now) throw new Error('Choose a start later today.');
  const minutes = (end - start) / 60000;
  if (minutes < 15 || minutes > 960) throw new Error('Choose 15 minutes to 16 hours, ending today.');
  const localTimestamp = (date) => {
    const pad = (n) => String(n).padStart(2, '0');
    const offset = -date.getTimezoneOffset();
    const zone = `${offset >= 0 ? '+' : '-'}${pad(Math.floor(Math.abs(offset) / 60))}:${pad(Math.abs(offset) % 60)}`;
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:00${zone}`;
  };
  return { startAt: localTimestamp(start), endAt: localTimestamp(end) };
}

const clock = (value) => new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
const get = (id) => document.getElementById(id);

function renderBlocks(host, blocks) {
  host.replaceChildren();
  for (const block of blocks) {
    const row = document.createElement('li'), time = document.createElement('span'), content = document.createElement('div');
    time.className = 'session-block-time';
    time.textContent = `${clock(block.startAt)}–${clock(block.endAt)}`;
    const title = document.createElement('strong'); title.textContent = block.title;
    content.append(title);
    if (block.rationale) { const p = document.createElement('p'); p.textContent = block.rationale; content.append(p); }
    row.append(time, content); host.append(row);
  }
}

export function mountSessionPlanner({ invoke }) {
  let proposal = null, busy = false, expiryTimer;
  const panel = get('sessionPlanner'), form = get('sessionPlanForm'), status = get('sessionPlanStatus');
  const intention = get('sessionIntention'), start = get('sessionStart'), end = get('sessionEnd'), feedback = get('sessionFeedback');
  function message(text, error = false) { status.textContent = text; status.dataset.state = error ? 'error' : ''; }
  function lock(value) {
    busy = value;
    for (const id of ['sessionIntention', 'sessionStart', 'sessionEnd', 'sessionFeedback', 'sessionGenerate', 'sessionRevise', 'sessionConfirm', 'sessionCancel']) get(id).disabled = value;
    get('sessionConfirm').disabled = value || !proposal;
    get('sessionGenerate').textContent = value ? 'Planning on this device…' : 'Suggest my session';
    form.setAttribute('aria-busy', String(value));
  }
  async function discard() {
    clearTimeout(expiryTimer);
    const old = proposal; proposal = null;
    get('sessionProposal').hidden = true; get('sessionConfirm').disabled = true;
    if (old) await invoke('cancel_session_plan', { id: old.id });
  }
  async function plan(revise) {
    if (busy) return;
    if (revise && (!proposal || !feedback.value.trim())) { message('Describe what you want to change.', true); feedback.focus(); return; }
    try {
      const request = { intention: intention.value.trim(), ...sessionWindow(start.value, end.value) };
      if (!request.intention) throw new Error('Describe what you want to work on today.');
      if (!revise) await discard();
      lock(true); message('The local agent is considering your available hours, tasks and saved preferences…');
      proposal = await invoke('propose_session_plan', { request, previousId: revise ? proposal.id : null, feedback: revise ? feedback.value.trim() : null });
      get('sessionProposal').hidden = false; get('sessionPlanSummary').textContent = proposal.summary;
      renderBlocks(get('sessionPlanBlocks'), proposal.blocks);
      const overflow = get('sessionUnscheduled'); overflow.replaceChildren(); overflow.hidden = !proposal.unscheduled.length;
      if (proposal.unscheduled.length) {
        const heading = document.createElement('strong'); heading.textContent = 'Work that needs more time'; overflow.append(heading);
        for (const text of proposal.unscheduled) { const p = document.createElement('p'); p.textContent = text; overflow.append(p); }
      }
      feedback.value = ''; message('Draft ready. Review the times and estimates before adding it to your local calendar.');
      clearTimeout(expiryTimer);
      expiryTimer = setTimeout(() => { proposal = null; get('sessionConfirm').disabled = true; message('This draft expired. Suggest a fresh session.', true); }, proposal.expiresInSeconds * 1000);
    } catch (error) { message(`${String(error)} Edit the request and try again.`, true); }
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
  get('sessionConfirm').addEventListener('click', async () => {
    if (busy || !proposal) return;
    lock(true);
    try {
      const events = await invoke('confirm_session_plan', { id: proposal.id });
      proposal = null; clearTimeout(expiryTimer); get('sessionProposal').hidden = true;
      message(`${events.length} blocks added to your FlowSight calendar.`); await refreshCalendar();
    } catch (error) { message(String(error), true); }
    finally { lock(false); }
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
      const events = (data.events || []).filter((e) => !e.provider && new Date(e.startAt).toDateString() === now.toDateString() && new Date(e.endAt) > now)
        .sort((a, b) => Date.parse(a.startAt) - Date.parse(b.startAt));
      get('sessionCalendar').hidden = !events.length; renderBlocks(get('sessionCalendarBlocks'), events);
    } catch (_) { /* Refresh again when the agent becomes available. */ }
  }
  return { open, refreshCalendar };
}
