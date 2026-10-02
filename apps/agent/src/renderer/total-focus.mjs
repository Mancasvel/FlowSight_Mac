import { html, t, setText, getLocale, localizeStatus } from './i18n.mjs';

const escape = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const defaults = {patterns:['instagram.com','tiktok.com','x.com'],exceptions:[],durationMinutes:50};
const sites = value => value.split(/[\n,]+/).map(item => item.trim()).filter(Boolean);

export function focusFields(prefix, preferences = defaults) {
  return html`<div class="total-focus-fields">
    <label for="${prefix}Sites">Pages to block</label>
    <textarea id="${prefix}Sites" class="input" rows="3" maxlength="5000" spellcheck="false" placeholder="instagram.com, tiktok.com, youtube.com/shorts">${escape(preferences.patterns.join('\n'))}</textarea>
    <p class="session-help">One domain or path per line. Subdomains are included.</p>
    <label for="${prefix}Exceptions">Allowed exceptions</label>
    <textarea id="${prefix}Exceptions" class="input" rows="2" maxlength="5000" spellcheck="false" placeholder="youtube.com/watch">${escape(preferences.exceptions.join('\n'))}</textarea>
    <p class="session-help">Exceptions take priority. Keep the pages you need for your work.</p>
    <label for="${prefix}Minutes">Session duration (minutes)</label>
    <input id="${prefix}Minutes" class="input" type="number" min="5" max="180" step="1" value="${preferences.durationMinutes}">
  </div>`;
}

export function readFocusFields(prefix) {
  return {patterns:sites(document.getElementById(`${prefix}Sites`).value),
    exceptions:sites(document.getElementById(`${prefix}Exceptions`).value),
    durationMinutes:Number(document.getElementById(`${prefix}Minutes`).value)};
}

export function focusExample() {
  return html`<figure class="total-focus-example" aria-label="Example of total focus">
    <div><span>instagram.com</span><strong>Page blocked</strong></div>
    <div><span>campus.example.edu</span><strong>Available for work</strong></div>
    <figcaption>Example · protect the pages you choose, keep your work accessible.</figcaption>
  </figure>`;
}

export function messagingFuture() {
  return html`<div class="total-focus-future"><strong>Messaging replies</strong><span>Coming later</span>
    <p>A future integration could reply that you are in deep focus and optionally share your task. No automatic replies are sent.</p></div>`;
}

export function mountTotalFocus({invoke}) {
  const host = document.getElementById('totalFocusSettings');
  host.innerHTML = html`<div class="card-header"><div class="card-title">Total focus</div></div>
    <p class="profile-card-intro">Block distracting websites with Browser Controls in Arc on Windows or macOS, and Chrome on Windows, macOS, or Linux.</p>
    <p class="session-help">FlowSight focus reminders are held in your local digest during this session.</p>
    <p id="totalFocusStatus" class="total-focus-status" role="status" aria-live="polite">Checking browser protection…</p>
    <p id="totalFocusActiveTask" class="total-focus-task" data-user-content></p>
    <label for="totalFocusTask">Your focus task</label><input id="totalFocusTask" class="input" type="text" maxlength="160" placeholder="What are you working on?">
    <div id="totalFocusConfig">${focusFields('totalFocus')}</div>
    <div class="total-focus-actions"><button type="button" class="button button-primary" id="totalFocusStart">Start total focus</button>
    <button type="button" class="button button-secondary" id="totalFocusEnd" hidden>End total focus</button>
    <button type="button" class="button button-secondary" id="totalFocusSave">Save settings</button></div>
    <p id="totalFocusFeedback" class="session-help" role="status" aria-live="polite"></p>
    <button type="button" class="button button-ghost" id="totalFocusBrowser">Connect your browser</button>
    <p class="session-help">Protection lasts until the chosen end time, even if FlowSight closes. You can end it from a blocked page. Tracking remains a separate choice.</p>
    ${messagingFuture()}`;
  let state = null, busy = false, dirty = false;
  const feedback = value => setText(document.getElementById('totalFocusFeedback'), value);
  const buttons = ['totalFocusStart','totalFocusEnd','totalFocusSave'];
  function render() {
    const session = state?.session;
    const active = session && Date.parse(session.expiresAt) > Date.now();
    const acknowledged = active && state.browser?.connected && state.browser?.fresh && state.browser?.applied && state.browser?.sessionId === session.id;
    const released = !active && state?.browser?.fresh && state?.browser?.applied;
    setText(document.getElementById('totalFocusStatus'), () => acknowledged ? t('Total focus active · browser block confirmed')
      : active ? t('Session active · browser protection not confirmed. Check the extension.')
      : released ? t('Session ended · waiting for the extension to release protection.')
      : state?.browser?.connected && !state.browser.totalFocusAvailable ? t('Browser connected. Update Browser Controls to enable total focus. In Arc, open arc://extensions and update the extension; approve any requested site access.')
      : state?.browser?.connected ? t('Browser connected · ready to start') : t('Connect Browser Controls to activate total focus.'));
    document.getElementById('totalFocusStatus').dataset.active = String(Boolean(acknowledged));
    setText(document.getElementById('totalFocusActiveTask'), () => active ? `${session.intention} · ${t('Until')} ${new Date(session.expiresAt).toLocaleTimeString(getLocale(),{hour:'2-digit',minute:'2-digit'})}` : '');
    document.getElementById('totalFocusStart').hidden = Boolean(active);
    document.getElementById('totalFocusEnd').hidden = !active;
    document.getElementById('totalFocusTask').disabled = Boolean(active) || busy;
    document.querySelectorAll('#totalFocusConfig input, #totalFocusConfig textarea').forEach(field => field.disabled = Boolean(active) || busy);
    for (const id of buttons) document.getElementById(id).disabled = busy || (id === 'totalFocusSave' && Boolean(active));
    document.getElementById('totalFocusStart').disabled = busy || !state?.browser?.connected || !state?.browser?.totalFocusAvailable;
    setText(document.getElementById('todayTotalFocusLabel'), () => active ? t('Total focus active') : t('Total focus'));
  }
  async function refresh(fill = false) {
    try {
      state = await invoke('get_total_focus');
      if (fill && !dirty && state?.preferences) {
        for (const [suffix,value] of [['Sites',state.preferences.patterns.join('\n')],['Exceptions',state.preferences.exceptions.join('\n')],['Minutes',state.preferences.durationMinutes]]) document.getElementById(`totalFocus${suffix}`).value = value;
      }
      render();
    } catch { feedback(() => t('Could not load focus settings. Try again.')); }
    return state;
  }
  async function action(run) {
    if (busy) return;
    busy = true; render();feedback(() => t('Applying browser protection…'));
    try { await run();await refresh(); }
    catch(error) { feedback(() => `${t('Could not update total focus:')} ${localizeStatus(error)}`); }
    finally { busy = false;render(); }
  }
  document.getElementById('totalFocusConfig').addEventListener('input',()=>{dirty=true;});
  document.getElementById('totalFocusSave').onclick = () => action(async()=>{
    await invoke('save_total_focus_preferences',{preferences:readFocusFields('totalFocus')});dirty=false;feedback(() => t('Focus settings saved'));
  });
  document.getElementById('totalFocusStart').onclick = () => action(async()=>{
    await invoke('start_total_focus',{intention:document.getElementById('totalFocusTask').value,preferences:readFocusFields('totalFocus')});
    feedback(() => t('Browser protection confirmed. Your session is ready.'));
  });
  document.getElementById('totalFocusEnd').onclick = () => action(async()=>{
    const result=await invoke('end_total_focus');feedback(() => result.browserReleased ? t('Total focus ended') : t('Session ended. Reconnect the extension or use End total focus on a blocked page to release it now.'));
  });
  document.getElementById('totalFocusBrowser').onclick=()=>{
    const details=document.getElementById('localAgentBrowserSetup');details.open=true;details.scrollIntoView({block:'start'});details.querySelector('summary').focus();
  };
  const open = ()=>{
    document.getElementById('navProfile').click();
    const task=document.getElementById('manualTask')?.value?.trim();
    if(task&&!state?.session)document.getElementById('totalFocusTask').value=task;
    host.scrollIntoView({block:'start'});document.getElementById('totalFocusTask').focus();refresh(true);
  };
  document.getElementById('todayTotalFocus').onclick=open;
  document.addEventListener('flowsight:languagechange',render);
  setInterval(()=>{ if(!document.hidden && (state?.session || document.getElementById('tabProfile')?.classList.contains('active'))) refresh(); },5000);
  refresh(true);
  return {refresh,preferences:()=>state?.preferences||structuredClone(defaults)};
}
