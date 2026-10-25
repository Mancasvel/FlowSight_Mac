import { t, html, getLocale } from './i18n.mjs';

export const escapeReviewText = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export function localDate(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`;
}
export function nextReviewDate(date = new Date()) { const next = new Date(date); next.setDate(next.getDate()+7); return localDate(next); }
const labels = { planned:'To review', tried:'Tried', not_tried:'Not tried', discarded:'Discarded' };
export function decisionDue(item, today=localDate()) { return item.status==='planned' && item.reviewDate<=today; }
function displayDate(value) { return /^\d{4}-\d{2}-\d{2}$/.test(value) ? new Date(value+'T12:00:00').toLocaleDateString(getLocale(),{day:'numeric',month:'short',year:'numeric'}) : value; }

export function reviewMarkup(items, model, draft=null) {
  const e=escapeReviewText;
  const history=items.map(item=>html`<li class="work-review-item">
    <p data-user-content>${e(item.text)}</p>
    <small>${e(t(labels[item.status]||'To review'))} · ${e(displayDate(item.reviewDate))}${decisionDue(item)?` · ${e(t('Ready to review'))}`:''}</small>
    ${item.note?html`<p class="work-review-note" data-user-content>${e(item.note)}</p>`:''}
    <button type="button" class="button button-ghost" data-review-edit="${e(item.id)}">Review or edit</button>
  </li>`).join('');
  const form=draft?html`<form class="work-review-form" id="reviewDecisionForm">
    <label for="reviewDecisionText">Your choice</label>
    <textarea id="reviewDecisionText" maxlength="500" required rows="3" data-user-content>${e(draft.text)}</textarea>
    <label for="reviewDecisionDate">Review on</label>
    <input id="reviewDecisionDate" type="date" required value="${e(draft.reviewDate)}">
    <label for="reviewDecisionStatus">What happened?</label>
    <select id="reviewDecisionStatus">${Object.entries(labels).map(([value,label])=>html`<option value="${value}" ${draft.status===value?'selected':''}>${e(t(label))}</option>`).join('')}</select>
    <label for="reviewDecisionNote">Result or context (optional)</label>
    <textarea id="reviewDecisionNote" maxlength="1000" rows="2" data-user-content>${e(draft.note)}</textarea>
    <div class="work-review-actions"><button type="submit" class="button button-primary">${e(t(draft.missing?'Save as new choice':'Save choice'))}</button><button type="button" class="button button-ghost" id="reviewCancelBtn">Cancel</button>
      ${draft.id&&!draft.missing?html`<button type="button" class="button button-ghost" id="reviewDeleteBtn">Delete choice</button>`:''}</div>
  </form>`:'';
  return html`<section class="work-review" aria-labelledby="workReviewTitle">
    <h3 id="workReviewTitle">Choose a change to try</h3>
    <p>Save one choice, then come back to review what happened. Keeping your current plan is a valid choice.</p>
    <p class="work-review-private">Saved only on this device. Not included in PDF, MCP or cloud sync. Your local retention setting also applies to these choices.</p>
    ${form || html`<div class="work-review-actions"><button type="button" class="button button-primary" id="reviewNewBtn">Write a choice</button><button type="button" class="button button-ghost" id="reviewKeepBtn">Keep my current plan</button></div>`}
    <p id="reviewDecisionMessage" role="status" aria-live="polite"></p>
    ${history?html`<details class="work-review-history" ${items.some(item=>decisionDue(item))?'open':''}><summary>${e(t('Saved choices'))} (${items.length})</summary><ul>${history}</ul></details>`:''}
  </section>`;
}

export function createWorkReviewController(invoke,{onError=()=>{}}={}) {
  let host=null,model=null,items=[],draft=null,baseline=null,busy=false,revision=0,historyOpen=false;
  const fields=value=>JSON.stringify([value?.text,value?.reviewDate,value?.status,value?.note]);
  function startDraft(value){draft=value;baseline=fields(value);}
  const readDraft=()=> {
    if(!draft||!host?.querySelector('#reviewDecisionText'))return;
    draft={...draft,text:host.querySelector('#reviewDecisionText').value,reviewDate:host.querySelector('#reviewDecisionDate').value,
      status:host.querySelector('#reviewDecisionStatus').value,note:host.querySelector('#reviewDecisionNote').value};
  };
  function message(value) { const node=host?.querySelector('#reviewDecisionMessage');if(node)node.textContent=value; }
  function canReplaceDraft() {readDraft();return !draft||fields(draft)===baseline||confirm(t('Discard your unsaved choice and open another?'));}
  function newChoice(kind='change',text='') {
    if(busy||!canReplaceDraft())return;
    startDraft({id:null,kind,text:kind==='keep'?t('Keep my current plan'):text,periodStart:model.periodStart||localDate(),periodEnd:model.periodEnd||localDate(),reviewDate:nextReviewDate(),status:'planned',note:''});
    render();host.querySelector('#reviewDecisionText')?.focus();
  }
  function render() {
    if(!host?.isConnected)return;
    historyOpen = Boolean(host.querySelector('.work-review-history')?.open || historyOpen);
    host.innerHTML=reviewMarkup(items,model,draft);
    const history=host.querySelector('.work-review-history');
    if(history){history.open=historyOpen||history.open;history.addEventListener('toggle',()=>{historyOpen=history.open;});}
    host.querySelector('#reviewNewBtn')?.addEventListener('click',()=>newChoice());
    host.querySelector('#reviewKeepBtn')?.addEventListener('click',()=>newChoice('keep'));
    host.querySelector('#reviewCancelBtn')?.addEventListener('click',()=>{if(busy)return;draft=null;render();host.querySelector('#reviewNewBtn')?.focus();});
    host.querySelectorAll('[data-review-edit]').forEach(button=>button.addEventListener('click',()=>{if(busy)return;if(draft?.id===button.dataset.reviewEdit){host.querySelector('#reviewDecisionText')?.focus();return;}if(!canReplaceDraft())return;startDraft({...items.find(item=>item.id===button.dataset.reviewEdit)});render();host.querySelector('#reviewDecisionText')?.focus();}));
    host.querySelector('#reviewDecisionForm')?.addEventListener('submit',async event=>{
      event.preventDefault();readDraft();
      if(!draft.text.trim()){message(t('Write a choice before saving.'));host.querySelector('#reviewDecisionText')?.focus();return;}
      const input={...draft};
      if(input.missing){input.id=null;delete input.missing;}
      await mutate('save_work_review_decision',{input});
    });
    host.querySelector('#reviewDeleteBtn')?.addEventListener('click',async()=>{
      if(busy||!confirm(t('Delete this saved choice?')))return;
      await mutate('delete_work_review_decision',{id:draft.id});
    });
  }
  async function mutate(command,args) {
    if(busy)return;busy=true;
    host.querySelectorAll('button,input,textarea,select').forEach(node=>node.disabled=true);message(t('Saving…'));
    try {
      const saved=await invoke(command,args);
      if(command==='delete_work_review_decision')items=items.filter(item=>item.id!==args.id);
      else items=[saved,...items.filter(item=>item.id!==saved.id)];
      draft=null;render();message(t(command==='delete_work_review_decision'?'Choice deleted':'Choice saved locally'));host.querySelector('#reviewNewBtn')?.focus();
    } catch(error) {
      if(command==='save_work_review_decision'&&String(error).includes('no longer available')) {
        draft={...draft,missing:true};
        items=items.filter(item=>item.id!==args.input.id);
        try {const loaded=await invoke('get_work_review_decisions');items=Array.isArray(loaded)?loaded:items;}catch(refreshError){onError(refreshError);}
        render();message(t('This saved choice is no longer available. Your draft is here; save it as a new choice.'));
      } else {
        host.querySelectorAll('button,input,textarea,select').forEach(node=>node.disabled=false);
        message(t(command==='delete_work_review_decision'?'Could not delete your choice. It is still here; try again.':'Could not save your choice. Your draft is still here; try again.'));
      }
      onError(error);
    }
    finally {busy=false;}
  }
  async function attach(nextHost,nextModel) {
    readDraft();historyOpen=Boolean(host?.querySelector('.work-review-history')?.open||historyOpen);host=nextHost;model=nextModel;const current=++revision;
    if(!host)return;
    render();
    if(busy)host.querySelectorAll('button,input,textarea,select').forEach(node=>node.disabled=true);
    try {const loaded=await invoke('get_work_review_decisions');if(current!==revision||busy)return;items=Array.isArray(loaded)?loaded:[];readDraft();if(!draft)render();}
    catch(error){if(current===revision){message(t('Could not load saved choices. Reopen the report to try again.'));onError(error);}}
  }
  function bindSuggestions(root) {
    root.querySelectorAll('[data-review-suggestion]').forEach(button=>button.addEventListener('click',()=>{
      newChoice('change',model.actions[Number(button.dataset.reviewSuggestion)]||'');host?.scrollIntoView({block:'nearest'});
    }));
  }
  return {attach,bindSuggestions};
}
