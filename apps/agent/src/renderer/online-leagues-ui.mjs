import {getLanguage} from './i18n.mjs';
const copy={
  en:{title:'Online',intro:'Focus with friends. Your work details stay private.',acceptTitle:'Online leagues, by your choice',notice:'When you accept, FlowSight sends your alias, league day and eligible focus minutes capped at 75 to the cloud. Friends see points and position in your private group. Apps, titles, tasks and captures are not sent to leagues. This sharing is separate from history sync and cloud AI.',rules:'25 min = 40 points · 50 min = 70 · 75 min = 100. Your five best days count each week. The league uses observed focus, not a judgment of productivity.',alias:'Name visible to friends',accept:'Accept online leagues',signin:'Sign in to FlowSight in Settings to participate.',start:'Points start from new observed activity after acceptance. Manual or imported time does not count.',active:'Online leagues enabled',points:'Points sent today',local:'Eligible minutes on this device',refresh:'Sync now',create:'Create private group',groupName:'Group name',join:'Join with an invitation',code:'Invitation code',joinButton:'Join group',invite:'Create invitation',copy:'Copy invitation',invitation:'Share this code with a friend. Valid for 7 days; creating another revokes the old code.',disable:'Disable leagues and erase league data',disableNote:'Stops sharing and leaves your groups. Groups you own are deleted. Your local work history stays available.',empty:'Create a group or join a friend to see the weekly ranking.',rank:'Weekly points',today:'Today',you:'You',pending:'Connecting…',failure:'Could not complete the request.',leave:'Leave group',deleteGroup:'Delete group',privacy:'What is shared',done:'Copied',record:'Start your next focus block from the central clock.',refreshNote:'Updates automatically while FlowSight is open. Results use Europe/Madrid calendar days.',withdrawError:'Sharing stopped locally. Cloud erasure will retry when the connection returns.'},
  es:{title:'Online',intro:'Concéntrate con amigos. El detalle de tu trabajo sigue privado.',acceptTitle:'Ligas online, cuando tú elijas',notice:'Al aceptar, FlowSight envía a la nube tu alias, el día de liga y los minutos de foco elegibles, limitados a 75. Tus amigos ven puntos y puesto dentro del grupo privado. Las aplicaciones, títulos, tareas y capturas no se envían a las ligas. Este permiso es independiente de sincronizar el historial y de la IA en la nube.',rules:'25 min = 40 puntos · 50 min = 70 · 75 min = 100. Cuentan tus cinco mejores días por semana. La liga usa foco observado; no juzga tu productividad.',alias:'Nombre visible para tus amigos',accept:'Aceptar ligas online',signin:'Inicia sesión en FlowSight desde Ajustes para participar.',start:'Los puntos empiezan con actividad observada nueva después de aceptar. El tiempo manual o importado no puntúa.',active:'Ligas online activadas',points:'Puntos enviados hoy',local:'Minutos elegibles en este dispositivo',refresh:'Sincronizar ahora',create:'Crear grupo privado',groupName:'Nombre del grupo',join:'Unirte con una invitación',code:'Código de invitación',joinButton:'Unirme al grupo',invite:'Crear invitación',copy:'Copiar invitación',invitation:'Comparte este código con un amigo. Válido 7 días; crear otro revoca el anterior.',disable:'Desactivar ligas y borrar sus datos',disableNote:'Detiene el envío y te saca de tus grupos. Se eliminan los grupos que has creado. Tu historial local de trabajo sigue disponible.',empty:'Crea un grupo o únete al de un amigo para ver la clasificación semanal.',rank:'Puntos semanales',today:'Hoy',you:'Tú',pending:'Conectando…',failure:'No se pudo completar la solicitud.',leave:'Salir del grupo',deleteGroup:'Eliminar grupo',privacy:'Qué se comparte',done:'Copiado',record:'Inicia tu próximo bloque de foco desde el reloj central.',refreshNote:'Se actualiza automáticamente mientras FlowSight está abierto. Los días de liga usan Europe/Madrid.',withdrawError:'El envío se ha detenido en este equipo. El borrado en la nube se reintentará cuando vuelva la conexión.'}
};
const errors={
  'The online league service is not deployed yet':'El servicio de ligas online aún no está desplegado.',
  'Online leagues could not connect. Check your connection and retry':'Sin conexión con las ligas. Se reintentará automáticamente.',
  'Invitation expired or unavailable':'La invitación ha caducado o ya no está disponible.',
  'Another device is scoring. Disable leagues on that device first':'Otro dispositivo está puntuando. Desactiva allí las ligas primero.',
  'This group already has 8 friends':'Este grupo ya tiene 8 amigos.',
  'Sign in again to use online leagues':'Vuelve a iniciar sesión para usar las ligas online.',
  'The league request could not be completed. Retry or sign in again':'No se pudo completar la solicitud. Reintenta o vuelve a iniciar sesión.',
  'Choose a display name of 2 to 32 characters':'Elige un alias de entre 2 y 32 caracteres.',
  'Scoring device does not match':'El dispositivo no coincide con el que está puntuando.',
  'Accept the online league service first':'Primero acepta el servicio de ligas online.',
  'Group limit reached':'Has llegado al límite de cinco grupos propios.',
  'Membership limit reached':'Has llegado al límite de ocho grupos.',
  'Invalid invitation code':'El código de invitación no es válido.',
  'Only the group owner can invite':'Solo quien ha creado el grupo puede generar invitaciones.',
  'Leagues were disabled or another device is scoring':'Las ligas se han desactivado o está puntuando otro dispositivo.',
  'Username already taken. Choose another':'Este username ya está ocupado. Elige otro.',
  'Use 3 to 20 letters, numbers, dots or underscores for your username':'Usa de 3 a 20 letras, números, puntos o guiones bajos. Empieza por una letra o un número.',
};
const consentCopy={
  es:{intro:'Grupos privados. Clasificación semanal.',acceptTitle:'Compite con tus amigos',benefit:'Convierte tus bloques de foco en puntos.',alias:'Tu alias',aliasHint:'Así te verán tus amigos.',pointsUnit:'puntos',weekly:'Cuentan tus <strong>5 mejores días</strong> de la semana.',cloudShort:'Al aceptar, envías a la nube <strong>tu username y tu foco diario</strong> (máx. 75 min). Tus amigos ven <strong>puntos y puesto</strong>.',privateShort:'<strong>Tu trabajo sigue privado:</strong> no se comparten aplicaciones, tareas, títulos ni capturas.',privacy:'Privacidad y reglas',startShort:'Puntúas desde que aceptas. Solo cuentan bloques de foco observado de al menos <strong>25 minutos</strong>. El tiempo manual o importado no suma.',serviceShort:'Se envían el día de liga y hasta 75 minutos elegibles. Este permiso es independiente del historial y de la IA en la nube. Puedes desactivarlo y borrar los datos de liga cuando quieras.'},
  en:{intro:'Private groups. Weekly ranking.',acceptTitle:'Compete with your friends',benefit:'Turn your focus blocks into points.',alias:'Your username',aliasHint:'Unique ? 3?20 letters, numbers, dots or _',pointsUnit:'points',weekly:'Your <strong>5 best days</strong> count each week.',cloudShort:'Accepting sends <strong>your username and daily focus</strong> to the cloud (max. 75 min). Friends see <strong>points and position</strong>.',privateShort:'<strong>Your work stays private:</strong> apps, tasks, titles and captures are not shared.',privacy:'Privacy and rules',startShort:'Points start when you accept. Only observed focus blocks of at least <strong>25 minutes</strong> count. Manual or imported time earns no points.',serviceShort:'The league day and up to 75 eligible minutes are sent. This choice is separate from history sync and cloud AI. You can disable leagues and erase league data at any time.'},
};
const escape=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const syncCopy={
  es:{automatic:'Sincronización automática',noFocus:'Los puntos empiezan con un bloque de foco de 25 min.',automaticNote:'Se envía en segundo plano. Si no hay conexión, se reintenta automáticamente.',cloudShort:'Al aceptar, envías automáticamente a la nube <strong>tu username y tu foco diario</strong> (máx. 75 min). Tus amigos ven <strong>puntos y puesto</strong>.'},
  en:{automatic:'Automatic sync',noFocus:'Points start with a 25-minute focus block.',automaticNote:'Sent in the background. Retries automatically when the connection returns.',cloudShort:'Accepting automatically sends <strong>your username and daily focus</strong> to the cloud (max. 75 min). Friends see <strong>points and position</strong>.'},
};
export function mountOnlineLeagues({root,invoke,listen}){
  let view={enabled:false,signed_in:false,groups:[]},busy=false,refreshTask=null,refreshAgain=false,revision=0,invitation='',error='',loaded=false;
  const t=()=>{const lang=getLanguage()==='es'?'es':'en';return {...copy[lang],...consentCopy[lang],...syncCopy[lang]};};
  const scoring=c=>`<dl class="online-scoring">${[[25,40],[50,70],[75,100]].map(([minutes,points])=>`<div><dt>${minutes} min</dt><dd><strong>${points}</strong> ${c.pointsUnit}</dd></div>`).join('')}</dl><p class="online-weekly">${c.weekly}</p>`;
  function updateSyncFeedback(){
    const c=t();
    const status=root.querySelector('.online-status');
    if(!status)return;
    status.dataset.state=busy?'pending':error?'error':view.synced_at&&view.enabled?'success':'idle';
    if(busy){status.textContent=c.pending;return;}
    if(error){status.textContent=getLanguage()==='es'?(errors[error]||error):error;return;}
    if(view.enabled){
      const date=view.synced_at?new Date(view.synced_at):null;
      const time=date&&!Number.isNaN(date.valueOf())?date.toLocaleTimeString(getLanguage()==='es'?'es-ES':'en-US',{hour:'2-digit',minute:'2-digit'}):'';
      status.innerHTML=`<strong>${c.automatic}</strong>${time?` · ${escape(time)}`:''}${Number(view.today_points||0)===0&&Number(view.eligible_minutes||0)===0?`<span class="online-sync-note">${c.noFocus}</span>`:''}`;
    }else status.textContent='';
  }
  function renderKeepingDrafts(){
    const focused=root.contains(document.activeElement)?document.activeElement:null;
    const focus=focused?.id,selection=focused?.tagName==='INPUT'?[focused.selectionStart,focused.selectionEnd]:null;
    const drafts=[...root.querySelectorAll('input:not([readonly])')].map(input=>[input.id,input.value]);
    const details=[...root.querySelectorAll('details')].map(detail=>[detail.className,detail.open]);
    render();
    for(const [id,value]of drafts){const input=root.querySelector(`#${id}`);if(input)input.value=value;}
    for(const [className,open]of details){const detail=[...root.querySelectorAll('details')].find(item=>item.className===className);if(detail)detail.open=open;}
    const input=focus?root.querySelector(`#${focus}`):null;
    input?.focus({preventScroll:true});
    if(selection&&input?.setSelectionRange)input.setSelectionRange(...selection);
  }
  function render(){
    const c=t();
    root.innerHTML=`<header class="online-heading"><h1>${c.title}</h1><p>${c.intro}</p></header>${!view.enabled?`
      <div class="online-status" role="status" aria-live="polite" aria-atomic="true"></div>
      <section class="online-consent"><h2>${c.acceptTitle}</h2><p class="online-benefit">${c.benefit}</p>${scoring(c)}
      <label for="onlineAlias">${getLanguage()==='es'?'Tu username':'Your username'}</label><input id="onlineAlias" class="input" minlength="3" maxlength="20" pattern="[a-zA-Z0-9][a-zA-Z0-9_.]{2,19}" autocomplete="username" autocapitalize="none" spellcheck="false" aria-describedby="onlineAliasHint" value="${escape(view.alias||'')}" placeholder="alex.foco"><p id="onlineAliasHint" class="online-alias-hint">${getLanguage()==='es'?'Único · 3–20 letras, números, puntos o _':'Unique · 3–20 letters, numbers, dots or _'}</p>
      <div class="online-consent-note"><p>${c.cloudShort}</p><p>${c.privateShort}</p></div>
      <button type="button" class="button button-primary online-accept" id="onlineAccept" ${busy||!view.signed_in?'disabled':''}>${c.accept}</button>${!view.signed_in?`<p>${c.signin}</p>`:''}
      <details class="online-consent-details"><summary>${c.privacy}</summary><p>${c.startShort}</p><p>${c.serviceShort}</p><p>${c.refreshNote}</p></details></section>`:`
      <section class="online-summary"><div class="online-summary-head"><h2>${c.active}</h2><span class="online-username">@${escape(view.alias||'')}</span></div>
      <div class="online-status" role="status" aria-live="polite" aria-atomic="true"></div>
      <div class="online-score-line"><span>${c.points}</span><strong>${Number(view.today_points||0)} / 100</strong></div><div class="online-score-line online-muted"><span>${c.local}</span><span>${Number(view.eligible_minutes||0)} / 75</span></div>${scoring(c)}</section>
      <div class="online-groups">${view.groups?.length?view.groups.map(g=>`<section class="online-group"><h2>${escape(g.name)}</h2><p>${c.rank} · ${escape(view.week_start||'')} · / 500</p><ol>${g.members.map(m=>`<li class="${m.mine?'online-mine':''}"><span>${Number(m.rank)}</span><span>@${escape(m.alias)}${m.mine?` <small>${c.you}</small>`:''}</span><strong>${Number(m.points)}</strong></li>`).join('')}</ol><div class="online-group-actions">${g.owner?`<button class="button button-ghost" data-action="invite" data-group="${escape(g.id)}" ${busy?'disabled':''}>${c.invite}</button>`:''}<button class="button button-ghost" data-action="leave" data-group="${escape(g.id)}" ${busy?'disabled':''}>${g.owner?c.deleteGroup:c.leave}</button></div></section>`).join(''):`<p class="online-empty">${c.empty}</p>`}</div>
      <section class="online-group-forms"><form id="onlineCreate"><label for="onlineGroupName">${c.groupName}</label><div class="online-form-line"><input class="input" id="onlineGroupName" required minlength="2" maxlength="48"><button class="button button-primary" ${busy?'disabled':''}>${c.create}</button></div></form>
      <form id="onlineJoin"><label for="onlineCode">${c.join}</label><input class="input" id="onlineCode" required maxlength="64" placeholder="${c.code}" autocomplete="off" spellcheck="false"><button class="button button-secondary" ${busy?'disabled':''}>${c.joinButton}</button></form></section>
      ${invitation?`<section class="online-invitation"><p>${c.invitation}</p><label for="onlineInvitationCode">${c.code}</label><input id="onlineInvitationCode" class="input" readonly value="${escape(invitation)}"><button class="button button-secondary" id="onlineCopy">${c.copy}</button></section>`:''}
      <details class="online-sharing"><summary>${c.privacy}</summary><p>${c.cloudShort}</p><p>${c.privateShort}</p><p>${c.startShort}</p><p>${c.automaticNote}</p><p>${c.serviceShort}</p><p>${c.refreshNote}</p><p>${c.disableNote}</p><button class="button button-secondary" id="onlineDisable" ${busy?'disabled':''}>${c.disable}</button></details>`}`;
    root.querySelector('#onlineAccept')?.addEventListener('click',()=>{const input=root.querySelector('#onlineAlias');const alias=input.value.trim().toLowerCase();view.alias=alias;if(!/^[a-z0-9][a-z0-9_.]{2,19}$/.test(alias)){error='Use 3 to 20 letters, numbers, dots or underscores for your username';render();return;}act(()=>invoke('set_online_league_consent',{accept:true,alias}));});
    root.querySelector('#onlineDisable')?.addEventListener('click',()=>act(()=>invoke('set_online_league_consent',{accept:false,alias:''}),true));
    root.querySelector('#onlineCreate')?.addEventListener('submit',e=>{e.preventDefault();const name=root.querySelector('#onlineGroupName').value.trim();act(()=>invoke('online_league_action',{request:{action:'create',name}}));});
    root.querySelector('#onlineJoin')?.addEventListener('submit',e=>{e.preventDefault();const code=root.querySelector('#onlineCode').value.trim();act(()=>invoke('online_league_action',{request:{action:'join',code}}));});
    root.querySelectorAll('[data-action]').forEach(b=>b.addEventListener('click',()=>act(()=>invoke('online_league_action',{request:{action:b.dataset.action,group_id:b.dataset.group}}))));
    root.querySelector('#onlineCopy')?.addEventListener('click',async e=>{try{await navigator.clipboard.writeText(invitation);e.target.textContent=c.done;}catch{root.querySelector('#onlineInvitationCode').select();}});
    updateSyncFeedback();
  }
  async function act(call,withdrawing=false){
    if(busy)return;revision++;busy=true;error='';render();
    try{const result=await call();if(result?.invitation_code)invitation=result.invitation_code;view={...view,...result};if(withdrawing)invitation='';error=result?.withdrawal_pending?t().withdrawError:result?.cloud_error||'';loaded=true;}
    catch(e){error=String(e);if(withdrawing){view.enabled=false;error=t().withdrawError;}}
    finally{busy=false;render();if(refreshAgain){refreshAgain=false;refresh();}}
  }
  function refresh(){
    if(busy)return Promise.resolve();
    if(refreshTask?.revision===revision){
      return refreshTask.promise;
    }
    const task={revision,promise:null};refreshTask=task;
    task.promise=(async()=>{
      try{
        const result=await invoke('get_online_leagues');
        if(task.revision!==revision)return;
        if(result?.account_id!==view.account_id){view={enabled:false,signed_in:false,groups:[]};invitation='';}
        error=result?.withdrawal_pending?t().withdrawError:result?.cloud_error||'';
        // An offline local response has no remote ranking; keep the last known scores.
        if(error&&result?.enabled){const {groups,today_points,week_start,...local}=result;view={...view,...local};}
        else view={...view,...result};
        if(!view.enabled)invitation='';
        loaded=true;
      }catch(e){if(task.revision===revision)error=String(e);}
      finally{
        if(refreshTask===task)refreshTask=null;
        if(task.revision===revision)renderKeepingDrafts();
        if(refreshAgain&&!busy){refreshAgain=false;refresh();}
      }
    })();
    return task.promise;
  }
  listen?.('online-leagues-updated',()=>{
    if(busy||refreshTask){refreshAgain=true;return;}
    refresh();
  }).catch(()=>{});
  document.addEventListener('flowsight:languagechange',render);
  document.addEventListener('visibilitychange',()=>{if(!document.hidden&&root.classList.contains('active'))refresh();});
  setInterval(()=>{if(root.classList.contains('active')&&!document.hidden&&!root.contains(document.activeElement))refresh();},60000);
  render();
  return {refresh,hasLoaded:()=>loaded};
}
