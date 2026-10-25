import { html, t, setText, getLocale, localizeStatus } from './i18n.mjs';

const escape = value => String(value ?? '').replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const defaults = {patterns:['instagram.com','tiktok.com','x.com'],exceptions:[],durationMinutes:50,quietNotifications:true,musicEnabled:false,musicUrl:''};
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
    <label class="total-focus-quiet" for="${prefix}Quiet"><input id="${prefix}Quiet" type="checkbox" ${preferences.quietNotifications !== false ? 'checked' : ''}> Silence Windows app notification banners</label>
    <p class="session-help">The previous notification setting is restored when the session ends. Browser protection continues if you quit FlowSight; notification silence resumes when you reopen it.</p>
    <label class="total-focus-quiet" for="${prefix}MusicEnabled"><input id="${prefix}MusicEnabled" type="checkbox" ${preferences.musicEnabled ? 'checked' : ''}> Open music when total focus starts</label>
    <label for="${prefix}MusicUrl">Music link</label>
    <input id="${prefix}MusicUrl" class="input total-focus-music-url" type="url" maxlength="2048" spellcheck="false" autocapitalize="off" autocomplete="off" aria-describedby="${prefix}MusicHelp" placeholder="https://music.youtube.com/playlist?list=…" value="${escape(preferences.musicUrl || '')}">
    <p id="${prefix}MusicHelp" class="session-help">Opens once in a background tab in your connected browser, keeping your current window in front. The browser may require you to allow audio playback on that site.</p>
  </div>`;
}

export function readFocusFields(prefix) {
  return {patterns:sites(document.getElementById(`${prefix}Sites`).value),
    exceptions:sites(document.getElementById(`${prefix}Exceptions`).value),
    durationMinutes:Number(document.getElementById(`${prefix}Minutes`).value),
    quietNotifications:document.getElementById(`${prefix}Quiet`).checked,
    musicEnabled:document.getElementById(`${prefix}MusicEnabled`).checked,
    musicUrl:document.getElementById(`${prefix}MusicUrl`).value.trim()};
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
    <p class="session-help">Total focus starts the main clock. Pause or resume both from the clock; your remaining focus time is kept.</p>
    <p class="session-help">FlowSight focus reminders are held in your local digest during this session.</p>
    <p id="totalFocusStatus" class="total-focus-status" role="status" aria-live="polite">Checking browser protection…</p>
    <p id="totalFocusQuietStatus" class="session-help" role="status"></p>
    <p id="totalFocusMusicStatus" class="session-help" role="status" hidden></p>
    <div id="totalFocusDigest" hidden>
      <strong>Reminders saved during focus</strong>
      <div id="totalFocusDigestItems" data-user-content></div>
      <button type="button" class="button button-secondary" id="totalFocusDigestDismiss">Dismiss saved reminders</button>
    </div>
    <div id="totalFocusRepair" hidden>
      <p class="session-help">Updating FlowSight does not update the browser extension. If the store still has the older version, use the compatible Browser Controls included with this app.</p>
      <button type="button" class="button button-secondary" id="totalFocusRepairOpen">Open compatible extension folder</button>
      <ol class="session-help">
        <li>Open your browser's extensions page: arc://extensions, chrome://extensions, or edge://extensions.</li>
        <li>Turn off the older FlowSight Browser Controls, enable Developer mode, and choose Load unpacked. Select the folder opened above.</li>
        <li>Open the new extension's options, paste your pairing key, and choose Save and connect.</li>
      </ol>
    </div>
    <p id="totalFocusActiveTask" class="total-focus-task" data-user-content></p>
    <label for="totalFocusTask">Your focus task (optional)</label><input id="totalFocusTask" class="input" type="text" maxlength="160" aria-describedby="totalFocusTaskError" placeholder="What are you working on?">
    <p id="totalFocusTaskError" class="session-help total-focus-error" role="alert" hidden></p>
    <div id="totalFocusConfig">${focusFields('totalFocus')}</div>
    <p id="totalFocusFeedback" class="session-help" role="status" aria-live="polite" hidden></p>
    <div class="total-focus-actions"><button type="button" class="button button-primary" id="totalFocusStart">Start total focus</button>
    <button type="button" class="button button-secondary" id="totalFocusEnd" hidden>End total focus</button>
    <button type="button" class="button button-secondary" id="totalFocusClock" hidden>Open main clock</button>
    <button type="button" class="button button-secondary" id="totalFocusSave">Save settings</button></div>
    <p class="session-help">Starting total focus saves these settings for the Today shortcut.</p>
    <button type="button" class="button button-ghost" id="totalFocusBrowser">Connect your browser</button>
    <p class="session-help">Protection lasts until the chosen end time, even if FlowSight closes. You can end it from a blocked page. Tracking remains a separate choice.</p>
    ${messagingFuture()}`;
  let state = null, busy = false, pending = '', dirty = false, digestKey = '', feedbackSurface = 'settings';
  const feedbackElement = () => document.getElementById(feedbackSurface === 'today' ? 'todayTotalFocusFeedback' : 'totalFocusFeedback');
  const feedback = (value, reveal = false) => {
    const element = feedbackElement();
    setText(element, value);element.hidden = !element.textContent;
    if (reveal) element.scrollIntoView({block:'nearest'});
  };
  const buttons = ['totalFocusStart','totalFocusEnd','totalFocusSave'];
  function render() {
    const session = state?.session;
    const paused = Boolean(session?.pausedAt);
    const active = session && !paused && Date.parse(session.expiresAt) > Date.now();
    const present = Boolean(active || paused);
    const acknowledged = active && state.browser?.connected && state.browser?.fresh && state.browser?.applied && state.browser?.sessionId === session.id;
    const released = !active && state?.browser?.fresh && state?.browser?.applied;
    setText(document.getElementById('totalFocusStatus'), () => paused ? t('Total focus paused · resume from the main clock') : acknowledged ? t('Total focus active · browser block confirmed')
      : active ? t('Session active · browser protection not confirmed. Check the extension.')
      : released ? t('Session ended · waiting for the extension to release protection.')
      : state?.browser?.connected && !state.browser.totalFocusAvailable ? t('Browser connected · website protection unavailable. Check site access or use the compatible extension below.')
      : state?.browser?.connected ? t('Browser connected · ready to start') : t('Connect Browser Controls to activate total focus.'));
    const musicUnsupported = document.getElementById('totalFocusMusicEnabled').checked && !state?.browser?.musicAvailable;
    document.getElementById('totalFocusRepair').hidden = present || !state?.browser?.connected || Boolean(state.browser.totalFocusAvailable && !musicUnsupported);
    document.getElementById('totalFocusStatus').dataset.active = String(Boolean(acknowledged));
    setText(document.getElementById('totalFocusQuietStatus'), () => state?.systemNotificationsQuiet
      ? t('Windows app notification banners are silenced.')
      : active && session.quietNotifications && state?.systemNotificationsAvailable
      ? t('Notification silence is not confirmed. End the session and try again.') : '');
    const musicStatus = document.getElementById('totalFocusMusicStatus');
    setText(musicStatus, () => session?.musicWarning
      ? `${t('Focus is active, but the music link could not be opened:')} ${localizeStatus(session.musicWarning)}`
      : !present && document.getElementById('totalFocusMusicEnabled').checked && state?.browser?.connected && !state.browser.musicAvailable
      ? t('Update Browser Controls to open music in the background.') : '');
    musicStatus.hidden = !musicStatus.textContent;
    const digest=state?.digest || [];
    document.getElementById('totalFocusDigest').hidden=!digest.length;
    const key=JSON.stringify(digest);
    if (key!==digestKey) {
      digestKey=key;const items=document.getElementById('totalFocusDigestItems');items.replaceChildren();
      for(const item of digest) { const p=document.createElement('p');p.textContent=`${item.title}: ${item.body}`;items.append(p); }
    }
    setText(document.getElementById('totalFocusActiveTask'), () => paused ? `${session.intention === 'General' ? t('General') : session.intention} · ${t('Paused')}` : active ? `${session.intention === 'General' ? t('General') : session.intention} · ${t('Until')} ${new Date(session.expiresAt).toLocaleTimeString(getLocale(),{hour:'2-digit',minute:'2-digit'})}` : '');
    document.getElementById('totalFocusStart').hidden = present && pending !== 'start';
    document.getElementById('totalFocusEnd').hidden = !present || pending === 'start';
    document.getElementById('totalFocusClock').hidden = !present;
    setText(document.getElementById('totalFocusStart'), () => pending === 'start' ? t('Starting total focus…') : t('Start total focus'));
    setText(document.getElementById('totalFocusEnd'), () => pending === 'end' ? t('Ending total focus…') : t('End total focus'));
    setText(document.getElementById('totalFocusSave'), () => pending === 'save' ? t('Saving settings…') : t('Save settings'));
    host.setAttribute('aria-busy', String(busy));
    document.getElementById('totalFocusTask').disabled = present || busy;
    document.querySelectorAll('#totalFocusConfig input, #totalFocusConfig textarea').forEach(field => field.disabled = present || busy);
    for (const id of buttons) document.getElementById(id).disabled = busy || (id === 'totalFocusSave' && present);
    document.getElementById('totalFocusStart').disabled = busy || !state?.browser?.connected || !state?.browser?.totalFocusAvailable;
    setText(document.getElementById('todayTotalFocusLabel'), () => t('Total focus settings'));
    const quickStart = document.getElementById('todayTotalFocusStart');
    quickStart.hidden = present && pending !== 'start';
    quickStart.disabled = busy || !state;
    quickStart.setAttribute('aria-busy', String(busy && pending === 'start'));
    setText(document.getElementById('todayTotalFocusStartLabel'), () => pending === 'start' ? t('Starting total focus…') : t('Start total focus'));
    document.dispatchEvent(new CustomEvent('flowsight:total-focus-state',{detail:{state,busy,pending}}));
  }
  async function refresh(fill = false) {
    try {
      state = await invoke('get_total_focus');
      if (fill && !dirty && state?.preferences) {
        for (const [suffix,value] of [['Sites',state.preferences.patterns.join('\n')],['Exceptions',state.preferences.exceptions.join('\n')],['Minutes',state.preferences.durationMinutes]]) document.getElementById(`totalFocus${suffix}`).value = value;
        document.getElementById('totalFocusQuiet').checked=state.preferences.quietNotifications !== false;
        document.getElementById('totalFocusMusicEnabled').checked=Boolean(state.preferences.musicEnabled);
        document.getElementById('totalFocusMusicUrl').value=state.preferences.musicUrl || '';
      }
      render();
    } catch { feedback(() => t('Could not load focus settings. Try again.')); }
    return state;
  }
  async function action(run, operation = '', surface = 'settings') {
    if (busy) return;
    feedbackSurface = surface;
    busy = true;pending = operation;render();
    feedback(() => operation === 'start' ? t('Starting total focus. Waiting for your browser to confirm protection; this can take up to 90 seconds.')
      : operation === 'end' ? t('Ending total focus…') : t('Saving settings…'), true);
    try { await run();await refresh(); }
    catch(error) { feedback(() => `${t('Could not update total focus:')} ${localizeStatus(error)}`); }
    finally { busy = false;pending = '';render();feedbackElement().scrollIntoView({block:'nearest'}); }
  }
  async function startWithPreferences(intention, preferences) {
    // Commit the exact fields used to start so Today and later sessions agree.
    const saved = await invoke('save_total_focus_preferences',{preferences});
    state.preferences = saved;dirty = false;
    return invoke('start_total_focus',{intention,preferences:saved});
  }
  const task = document.getElementById('totalFocusTask');
  task.addEventListener('input',()=>{
    task.removeAttribute('aria-invalid');document.getElementById('totalFocusTaskError').hidden = true;
  });
  document.getElementById('totalFocusConfig').addEventListener('input',()=>{dirty=true;render();});
  document.getElementById('totalFocusSave').onclick = () => action(async()=>{
    await invoke('save_total_focus_preferences',{preferences:readFocusFields('totalFocus')});dirty=false;feedback(() => t('Focus settings saved'));
  }, 'save');
  document.getElementById('totalFocusStart').onclick = () => {
    if (busy) return;
    feedbackSurface = 'settings';
    // Validate before disabling controls, so focus and the error remain visible.
    if (!task.checkValidity() || task.value.trim().length > 160) {
      const error = document.getElementById('totalFocusTaskError');
      setText(error, () => t('Keep your focus task within 160 characters.'));error.hidden = false;
      task.setAttribute('aria-invalid','true');feedback('');task.focus();task.scrollIntoView({block:'center'});return;
    }
    const minutes = document.getElementById('totalFocusMinutes');
    if (!minutes.checkValidity()) {
      feedback(() => t('Choose a focus duration between 5 and 180 minutes.'), true);
      minutes.focus();minutes.scrollIntoView({block:'center'});minutes.reportValidity();return;
    }
    const intention = task.value.trim(), preferences = readFocusFields('totalFocus');
    return action(async()=>{
      const result = await startWithPreferences(intention,preferences);
      feedback(() => result?.session?.musicWarning ? `${t('Focus is active, but the music link could not be opened:')} ${localizeStatus(result.session.musicWarning)}` : t('Browser protection confirmed. Your session is ready.'));
    }, 'start');
  };
  document.getElementById('totalFocusEnd').onclick = () => action(async()=>{
    const result=await invoke('end_total_focus');feedback(() => result.notificationWarning ? `${t('Could not restore notification banners:')} ${localizeStatus(result.notificationWarning)}` : result.browserReleased ? t('Total focus ended') : t('Session ended. Reconnect the extension or use End total focus on a blocked page to release it now.'));
  }, 'end');
  document.getElementById('totalFocusClock').onclick=()=>document.getElementById('navToday').click();
  document.getElementById('totalFocusDigestDismiss').onclick=()=>action(async()=>{
    await invoke('dismiss_total_focus_digest',{ids:(state?.digest||[]).map(item=>item.id)});
    feedback(() => t('Saved reminders dismissed'));
  });
  document.getElementById('totalFocusBrowser').onclick=()=>{
    const details=document.getElementById('localAgentBrowserSetup');details.open=true;details.scrollIntoView({block:'start'});details.querySelector('summary').focus();
  };
  document.getElementById('totalFocusRepairOpen').onclick = async () => {
    try { await invoke('open_browser_extension_folder'); }
    catch(error) { feedback(() => `${t('Could not open the extension folder:')} ${localizeStatus(error)}`); }
  };
  const open = ()=>{
    feedbackSurface = 'settings';
    document.getElementById('navProfile').click();
    const task=document.getElementById('manualTask')?.value?.trim();
    if(task&&!state?.session)document.getElementById('totalFocusTask').value=task;
    host.scrollIntoView({block:'start'});document.getElementById('totalFocusTask').focus();refresh(true);
  };
  document.getElementById('todayTotalFocus').onclick=open;
  document.addEventListener('flowsight:languagechange',render);
  setInterval(()=>{ if(!document.hidden && (state?.session || document.getElementById('tabProfile')?.classList.contains('active'))) refresh(); },5000);
  refresh(true);
  const clockAction=async command=>{
    if(busy)throw new Error(t('Wait for the current focus action to finish.'));
    busy=true;pending=command==='resume_total_focus'?'start':'end';render();
    try { const result=await invoke(command);await refresh();return result; }
    finally {busy=false;pending='';render();}
  };
  const startFromToday = intention => action(async () => {
    await refresh();
    if (state?.session) throw new Error(t('Total focus is already active. End it before starting another session.'));
    if (!state?.browser?.connected || !state.browser.totalFocusAvailable) {
      throw new Error(t('Connect Browser Controls in total focus settings, then try again.'));
    }
    const preferences = dirty ? readFocusFields('totalFocus') : structuredClone(state.preferences || defaults);
    const result = await startWithPreferences(String(intention || '').trim(),preferences);
    feedback(() => result?.session?.musicWarning ? `${t('Focus is active, but the music link could not be opened:')} ${localizeStatus(result.session.musicWarning)}` : t('Browser protection confirmed. Your session is ready.'));
  }, 'start', 'today');
  return {refresh,session:()=>state?.session,busy:()=>busy,ready:()=>Boolean(state),clockAction,startFromToday,preferences:()=>state?.preferences||structuredClone(defaults)};
}
