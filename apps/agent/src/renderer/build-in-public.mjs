import {getLanguage} from './i18n.mjs';

const esc=value=>String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const tr=(es,en)=>getLanguage()==='es'?es:en;
const channelName={linkedin:'LinkedIn',x:'X',instagram:'Instagram'};
const limits={linkedin:3000,x:280,instagram:2200};
const dateLabel=value=>new Date(`${value}T12:00:00`).toLocaleDateString(getLanguage()==='es'?'es-ES':'en-GB',{day:'numeric',month:'short'});
export function contentLength(text,channel){
  if(channel!=='x')return [...text].length;
  const urls=text.match(/https?:\/\/\S+/g)||[];
  return urls.length*23+[...text.replace(/https?:\/\/\S+/g,'')].reduce((n,c)=>{const p=c.codePointAt(0);return n+(p<=0x10ff||(p>=0x2000&&p<=0x200d)||(p>=0x2010&&p<=0x201f)||(p>=0x2032&&p<=0x2037)?1:2);},0);
}
export function drawContentImage(visual,canvas=document.createElement('canvas')){
  canvas.width=1080;canvas.height=1350;
  const ctx=canvas.getContext('2d');
  ctx.fillStyle='#f6f8f5';ctx.fillRect(0,0,1080,1350);
  function wrap(text,y,size,weight,color,maxLines){
    let lines=[];
    do {
      ctx.font=`${weight} ${size}px Manrope, sans-serif`;lines=[''];
      for(const word of String(text).split(/\s+/)){
        const index=lines.length-1,test=lines[index]?`${lines[index]} ${word}`:word;
        if(ctx.measureText(test).width<=920)lines[index]=test;
        else {
          if(lines[index])lines.push('');
          for(const char of word){const last=lines.length-1;if(ctx.measureText(lines[last]+char).width>920)lines.push(char);else lines[last]+=char;}
        }
      }
      if(lines.length<=maxLines)break;
      size-=2;
    }while(size>=20);
    ctx.fillStyle=color;
    for(const line of lines){ctx.fillText(line,76,y);y+=size*1.28;}
    return y;
  }
  const end=wrap(visual.title,240,72,750,'#152d2a',4);
  wrap(visual.detail,Math.max(600,end+62),43,450,'#41534e',8);
  ctx.fillStyle='#c6d4cd';ctx.fillRect(76,1196,928,2);
  ctx.font='500 28px Manrope, sans-serif';ctx.fillStyle='#47675e';ctx.fillText(String(visual.footer||'').slice(0,80),76,1250,928);
  return canvas.toDataURL('image/png');
}

export function mountBuildInPublic({root,invoke,canUse=()=>true}){
  const nativeInvoke=invoke;
  invoke=(command,args)=>canUse()?nativeInvoke(command,args):Promise.reject(tr('Esta función pertenece al perfil individual.','This feature belongs to the individual profile.'));
  const emptyStudio=owner_id=>({owner_id,settings:{project_name:'FlowSight',voice:'',excluded_terms:[],cadence:'manual',topic_direction:''},evidence:[],briefs:[],drafts:[],profiles:[],styles:{},tokens:[]});
  let state=emptyStudio('device'),tab='stories',busy=false,error='',notice='',local=[],editor=null,days=7,loaded=false;
  let networks=['linkedin','x','instagram'],secret='',cadenceTimer=null,refreshing=false,pendingFocus=null;
  const editing=new Map();
  const formDrafts=new Map();
  let targets={},history=[],recentKeys=[],showHistory=false,setup=false,collecting=false,directionOpen=false,manualDraft=null,includeCodexPrompts=false,pendingDirection=null,localOwner=null,disposed=false;
  const localItems=()=>showHistory?history:local;
  const editorKey=value=>value.id||`local:${value.source_key}`;
  const ideaSignals=item=>item.signals||{};
  const styles=channel=>state?.styles?.[channel]||[];
  const styleLabel=(channel,id)=>{const item=styles(channel).find(item=>item.id===id);return item?.[getLanguage()==='es'?'es':'en']||id;};
  const connectionMessage=()=>tr('La conexión online no está disponible. Puedes seguir buscando y revisando ideas en tu equipo. Inicia sesión para guardar ideas online o conectar tu asistente.','The online connection is unavailable. You can keep finding and reviewing ideas on your device. Sign in to save ideas online or connect your assistant.');
  const cloudError=value=>/invalid_session|session.*(?:invalid|expired)|sign in.*settings/i.test(String(value))?tr('La sesión online ha caducado. Inicia sesión para guardar o compartir; la búsqueda de ideas en tu equipo sigue disponible.','The online session has expired. Sign in to save or share; finding ideas on your device is still available.'):String(value);
  const api=request=>{
    if(request.action!=='studio'&&!loaded)return Promise.reject(connectionMessage());
    return invoke('bip_cloud',{request:{...request,...(request.action!=='studio'?{expected_owner:state.owner_id}:{})}});
  };
  const localKey=()=>`bip-local-ideas-${state.owner_id}`;
  function persistLocal(){try{localStorage.setItem(localKey(),JSON.stringify({evidence:local,history,recentKeys,manualDraft,includeCodexPrompts,settings:state.settings,pendingDirection,days,at:Date.now()}));}catch{/* Storage may be full. */}}
  function acceptStudio(next,{online=true}={}){
    if(localOwner!==next.owner_id){
      editor=null;secret='';local=[];history=[];recentKeys=[];manualDraft=null;includeCodexPrompts=false;pendingDirection=null;days=7;targets={};editing.clear();formDrafts.clear();showHistory=false;setup=false;
      try{const cached=JSON.parse(localStorage.getItem(`bip-local-ideas-${next.owner_id}`)||'null');if(cached&&Date.now()-cached.at<30*86400000){local=cached.evidence||[];history=cached.history||[];recentKeys=cached.recentKeys||[];manualDraft=cached.manualDraft||null;includeCodexPrompts=cached.includeCodexPrompts===true;pendingDirection=typeof cached.pendingDirection==='string'?cached.pendingDirection:null;if([1,7,30].includes(cached.days))days=cached.days;if(!online&&cached.settings)next.settings={...next.settings,...cached.settings};}}catch{/* Invalid cache is ignored. */}
      localOwner=next.owner_id;
    }
    if(pendingDirection!==null)next.settings.topic_direction=pendingDirection;
    state=next;loaded=online;
    for(const channel of Object.keys(channelName)){const profile=state.profiles?.find(item=>item.channel===channel);if(!targets[channel]?.style)targets[channel]={style:profile?.style||styles(channel)[0]?.id,profile_id:profile?.id||null};}
    persistLocal();
  }
  async function resolveLocalOwner(){
    // This command reads the stored identity only; it does not validate a JWT online.
    const user=await invoke('get_current_user');
    if(disposed)return;
    const owner=user?.user_id||'device';
    if(localOwner!==owner)acceptStudio(emptyStudio(owner),{online:false});
  }
  function saveLocalDirection(text){
    if(text!==state.settings.topic_direction)pendingDirection=text;
    state.settings.topic_direction=text;formDrafts.delete('bipDirectionForm');persistLocal();
  }
  async function syncDirection(next){
    if(pendingDirection===null||next.owner_id!==state.owner_id)return;
    const text=pendingDirection;
    try{
      await invoke('bip_cloud',{request:{action:'direction',text,expected_owner:next.owner_id}});
      if(pendingDirection===text){pendingDirection=null;next.settings.topic_direction=text;persistLocal();}
    }catch{/* Keep the local direction until the online connection recovers. */}
  }
  const controlsDisabled=()=>busy?'disabled':'';
  const selected=()=>state?.evidence.filter(item=>item.state==='selected')||[];
  const stateLabel=value=>({draft:tr('Borrador','Draft'),approved:tr('Aprobado','Approved'),sending:tr('Enviando','Sending'),scheduled:tr('En cola','Queued'),published:tr('Publicado','Published'),failed:tr('Revisar envío','Review delivery'),uncertain:tr('Comprobar en Buffer','Check Buffer')})[value]||value;
  async function refresh(){
    if(!canUse()||busy||refreshing)return;refreshing=true;
    captureEditor();
    cacheEditor();
    render();
    try{await localReady;await resolveLocalOwner();render();const next=await api({action:'studio'});if(!busy&&!disposed){await syncDirection(next);if(!busy&&!disposed){acceptStudio(next);error='';}}}catch{loaded=false;secret='';}finally{refreshing=false;}
    render();
  }
  async function run(task,{reload=true}={}){
    if(!canUse()||busy)return;busy=true;error='';notice='';render();
    try{await task();if(reload){const next=await api({action:'studio'});await syncDirection(next);acceptStudio(next);}}
    catch(e){error=cloudError(e);}
    finally{busy=false;render();}
  }
  function header(){return `<h1 class="bip-sr-only">Build in public</h1><nav class="bip-tabs" aria-label="Build in public">${[['stories',tr('Ideas','Ideas')],['prompts',tr('Prompts','Prompts')],['drafts',tr('Contenido','Content')],['connections',tr('Ajustes','Settings')]].map(([value,label])=>`<button type="button" data-view="${value}" aria-current="${tab===value?'page':'false'}" ${controlsDisabled()}>${label}${value==='drafts'&&state?.drafts.length?` <span>${state.drafts.length}</span>`:''}</button>`).join('')}</nav>`;}
  function signalLine(item){const info=ideaSignals(item),parts=[];if(info.recorded_seconds)parts.push(`${Math.max(1,Math.round(info.recorded_seconds/60))} min`);if(info.observations)parts.push(`${info.observations} ${tr('observaciones','observations')}`);parts.push(dateLabel(item.occurred_on));return parts.join(' · ');}
  function workExcerpt(item){const summary=item.summary||'';return (summary.split('CURRENT ACTION:')[1]?.split('PROGRESS:')[0]||summary.split('VISIBLE CONTENT:')[1]?.split('ARTIFACT TYPE:')[0]||summary).trim();}
  function ideaRows(items){return items.map((item,index)=>`<button class="bip-story" data-local="${index}" ${controlsDisabled()}><strong>${esc(item.title)}</strong><span class="bip-idea-excerpt">${esc(workExcerpt(item))}</span><span class="bip-story-meta">${esc(signalLine(item))}${ideaSignals(item).angle?` · ${esc(ideaSignals(item).angle)}`:''}</span></button>`).join('');}
  function stories(){if(setup)return setupPanel();const items=localItems();return `<div class="bip-discovery"><div class="bip-discovery-toolbar"><label class="bip-sr-only" for="bipPeriod">${tr('Periodo de trabajo','Work period')}</label><select id="bipPeriod" ${controlsDisabled()}>${[[1,tr('Hoy','Today')],[7,tr('Últimos 7 días','Last 7 days')],[30,tr('Últimos 30 días','Last 30 days')]].map(([n,label])=>`<option value="${n}" ${days===n?'selected':''}>${label}</option>`).join('')}</select><button class="button button-primary" data-action="collect" ${controlsDisabled()}>${collecting?tr('Buscando ideas…','Finding ideas…'):local.length||history.length?tr('Más ideas','More ideas'):tr('Buscar ideas','Find ideas')}</button></div>
    <details class="bip-direction" ${directionOpen?'open':''}><summary>${tr('¿Qué te gustaría contar?','What would you like to share?')}${state?.settings.topic_direction?` <span>${tr('Con enfoque','Direction set')}</span>`:''}</summary><form id="bipDirectionForm"><label for="bipDirection">${tr('Orienta la selección de temas','Guide topic selection')}</label><textarea id="bipDirection" rows="3" maxlength="1000" placeholder="${tr('Por ejemplo: decisiones de producto útiles para otros founders. Evita detalles de código.','For example: product decisions useful to other founders. Avoid code details.')}">${esc(state?.settings.topic_direction||'')}</textarea><p class="bip-note">${tr('Se combina con el tiempo registrado, la recurrencia y el interés del tema. Codex también puede ajustar este enfoque.','Combined with recorded time, recurrence and topic interest. Codex can also update this direction.')}</p><button class="button" type="submit" ${controlsDisabled()}>${tr('Guardar enfoque','Save direction')}</button></form></details></div>
    <label class="bip-choice"><input id="bipCodexPrompts" type="checkbox" ${includeCodexPrompts?'checked':''} ${controlsDisabled()}><span>${tr('Incluir mis prompts locales de Codex','Include my local Codex prompts')}<small>${tr('Solo tus mensajes del periodo elegido. Se leen en tu equipo; revisa una idea antes de guardarla online.','Only your messages from the selected period. Read on your device; review an idea before saving it online.')}</small></span></label>
    ${items.length?`<section class="bip-story-list"><div class="bip-section-heading"><h2>${showHistory?tr('Ideas anteriores','Earlier ideas'):`${items.length} ${tr('ideas para contar','ideas to share')}`}</h2>${selected().length?`<button class="bip-text-button" data-action="setup">${tr('Preparar','Prepare')} (${selected().length})</button>`:''}</div><p class="bip-selection-note">${tr('En tu equipo. Elige una idea para revisar los hechos.','On your device. Choose an idea to review its facts.')}</p>${ideaRows(items)}${history.length?`<button class="bip-text-button" data-action="history">${showHistory?tr('Volver a esta tanda','Back to this batch'):`${tr('Ver ideas anteriores','See earlier ideas')} (${history.length})`}</button>`:''}</section>`:`<div class="bip-empty"><h2>${tr('Tu trabajo tiene cosas que contar','Your work has stories to tell')}</h2><p>${tr('Busca ideas concretas a partir de tu trabajo. Puedes orientar la búsqueda con tus propias palabras.','Find concrete ideas from your work. Guide the selection in your own words.')}</p>${history.length?`<button class="bip-text-button" data-action="history">${tr('Ver ideas anteriores','See earlier ideas')} (${history.length})</button>`:''}</div>`}
    <section class="bip-saved-list"><div class="bip-section-heading"><h2>${tr('Tus ideas elegidas','Your chosen ideas')}${selected().length?` <span class="bip-count">${selected().length}</span>`:''}</h2><button class="bip-text-button" data-action="manual" ${controlsDisabled()}>${tr('Añadir hito','Add highlight')}</button></div>${state?.evidence.filter(item=>item.state!=='dismissed').length?state.evidence.filter(item=>item.state!=='dismissed').map(item=>`<article class="bip-saved-story"><label class="bip-choice"><input type="checkbox" data-select="${item.id}" ${item.state==='selected'?'checked':''} ${controlsDisabled()}><span><strong>${esc(item.title)}</strong><small>${dateLabel(item.occurred_on)} · ${item.confirmed?tr('Confirmado por ti','Confirmed by you'):tr('Pendiente de confirmar','Awaiting confirmation')}</small></span></label><button class="bip-text-button" data-evidence="${item.id}" ${controlsDisabled()}>${tr('Revisar','Review')}</button></article>`).join(''):`<p class="bip-note">${tr('Guarda aquí las que quieras desarrollar con tu IA.','Save the ideas you want to develop with your AI here.')}</p>`}${selected().length?`<button class="button button-primary bip-primary" data-action="setup">${tr('Preparar prompts','Prepare prompts')} · ${selected().length}</button>`:''}</section>`;}
  function setupPanel(){return `<section class="bip-generation"><button class="bip-back" data-action="cancel-setup">${tr("Volver a las ideas","Back to ideas")}</button><h2>${tr('Cómo quieres contarlo','How you want to tell it')}</h2><div class="bip-targets">${Object.entries(channelName).map(([channel,name])=>`<div class="bip-target"><label class="bip-choice"><input type="checkbox" data-network="${channel}" ${networks.includes(channel)?'checked':''} ${controlsDisabled()}><span>${name}</span></label>${networks.includes(channel)?`<label for="bipStyle-${channel}">${tr('Estilo','Style')}</label><select id="bipStyle-${channel}" data-style="${channel}" ${controlsDisabled()}>${styles(channel).map(style=>`<option value="${style.id}" ${targets[channel]?.style===style.id?'selected':''}>${esc(style[getLanguage()==='es'?'es':'en'])}</option>`).join('')}</select><label for="bipProfile-${channel}">${tr('Perfil','Profile')}</label><select id="bipProfile-${channel}" data-profile="${channel}" ${controlsDisabled()}><option value="">${tr('Lo elegiré con mi IA','I will choose with my AI')}</option>${(state?.profiles||[]).filter(profile=>profile.channel===channel).map(profile=>`<option value="${profile.id}" ${targets[channel]?.profile_id===profile.id?'selected':''}>${esc(profile.name)} · ${esc(profile.handle)}</option>`).join('')}</select>`:''}</div>`).join('')}</div><button class="button button-primary bip-primary" data-action="generate" ${busy||!networks.length||selected().length>3?'disabled':''}>${tr('Preparar prompts para mi IA','Prepare prompts for my AI')}</button><p class="bip-note">${selected().length>3?tr('Selecciona un máximo de 3 ideas por encargo.','Select up to 3 ideas per brief.'):tr('Cada prompt incluye tu trabajo, tu perfil y el estilo elegido. Tu IA se encarga de desarrollar el contenido.','Each prompt includes your work, profile and chosen style. Your AI develops the content.')}</p></section>`;}
  function prompts(){return state?.briefs?.length?`<p class="bip-note">${tr('Copia un prompt o pide a Codex que recoja tus encargos de FlowSight.','Copy a prompt or ask Codex to pick up your FlowSight briefs.')}</p>${state.briefs.map(brief=>`<button class="bip-draft-row" data-brief="${brief.id}"><strong class="bip-row-title">${esc(state.evidence.find(item=>item.id===brief.evidence_id)?.title||tr('Idea de trabajo','Work idea'))}</strong><div class="bip-draft-meta"><strong>${channelName[brief.channel]}</strong><span>${esc(styleLabel(brief.channel,brief.style))}</span></div><small>${(state.drafts||[]).some(draft=>draft.brief_id===brief.id)?tr('Tu IA ha devuelto contenido','Your AI returned content'):tr('Listo para tu IA','Ready for your AI')}</small></button>`).join('')}`:`<div class="bip-empty"><h2>${tr('Un buen prompt empieza con hechos','A useful prompt starts with evidence')}</h2><p>${tr('Elige una idea, una red y un estilo. FlowSight preparará el contexto para que tu IA haga el resto.','Choose an idea, network and style. FlowSight prepares the context for your AI.')}</p><button class="button" data-view="stories">${tr('Elegir una idea','Choose an idea')}</button></div>`;}
  function briefEditor(){const brief=editor.value;return `<button class="bip-back" data-action="back">← ${tr('Prompts','Prompts')}</button><div class="bip-section-heading"><h2>${esc(state.evidence.find(item=>item.id===brief.evidence_id)?.title||channelName[brief.channel])}</h2><span class="bip-state">${channelName[brief.channel]} · ${esc(styleLabel(brief.channel,brief.style))}</span></div>${editor.dirty?`<p class="bip-note">${tr('Cambios sin guardar','Unsaved changes')}</p>`:''}<p class="bip-note">${tr('Puedes ajustar el encargo antes de pasarlo a tu IA. Se guarda en tu espacio privado.','Adjust the brief before handing it to your AI. It stays in your private space.')}</p><form id="bipBriefEdit"><label for="bipPrompt">${tr('Prompt con tu contexto','Prompt with your context')}</label><textarea id="bipPrompt" rows="15" maxlength="14000" required>${esc(brief.prompt)}</textarea><button class="button bip-primary" type="submit" ${controlsDisabled()}>${tr('Guardar prompt','Save prompt')}</button></form><button class="button button-primary bip-primary" data-action="copy-brief">${tr('Copiar para mi IA','Copy for my AI')}</button><p class="bip-note">${tr('Con el MCP conectado, dile a Codex: «Recoge mis encargos de FlowSight y prepara las propuestas».','With MCP connected, ask Codex: “Pick up my FlowSight briefs and prepare the proposals.”')}</p><button class="bip-text-button" data-action="delete-brief">${tr('Eliminar encargo','Delete brief')}</button>`;}
  function drafts(){return state?.drafts.length?`<div class="bip-draft-list">${state.drafts.map(draft=>`<button class="bip-draft-row" data-draft="${draft.id}" ${controlsDisabled()}><strong class="bip-row-title">${esc(state.evidence.find(item=>item.id===draft.evidence_id)?.title||tr('Propuesta de contenido','Content proposal'))}</strong><div class="bip-draft-meta"><strong>${channelName[draft.channel]}</strong><span data-state="${draft.state}">${stateLabel(draft.state)}</span></div><p>${esc(draft.body.slice(0,180))}${draft.body.length>180?'…':''}</p><small>${draft.engine==='local'?tr('IA local','Local AI'):draft.engine==='assistant'?'Codex / MCP':tr('Escrito por ti','Written by you')} · ${tr('Versión','Version')} ${draft.revision}</small></button>`).join('')}</div>`:`<div class="bip-empty"><h2>${tr('Aquí vuelve el contenido de tu IA','Your AI content returns here')}</h2><p>${tr('Prepara un prompt y pídele a tu asistente que desarrolle la propuesta y la guarde en FlowSight.','Prepare a prompt, then ask your assistant to develop the proposal and save it in FlowSight.')}</p><button class="button" data-view="stories">${tr('Elegir una historia','Choose a story')}</button></div>`;}
  function connections(){const prefs=state?.settings||{project_name:'FlowSight',voice:'',excluded_terms:[],cadence:'manual'};return `<section class="bip-connection"><h2>${tr('Mis perfiles sociales','My social profiles')}</h2><p>${tr('Añade tus perfiles aquí para dar contexto a los prompts. No necesitas conectarlos a Buffer.','Add your profiles here to give prompts context. No Buffer connection is needed.')}</p>${(state?.profiles||[]).map(profile=>`<div class="bip-profile-row"><div><strong>${channelName[profile.channel]} · ${esc(profile.name)}</strong><small>${esc(profile.handle)} · ${esc(styleLabel(profile.channel,profile.style))}</small></div><button class="bip-text-button" data-delete-profile="${profile.id}">${tr('Quitar','Remove')}</button></div>`).join('')}<form id="bipProfileForm"><label for="bipProfileNetwork">${tr('Red social','Social network')}</label><select id="bipProfileNetwork">${Object.entries(channelName).map(([channel,name])=>`<option value="${channel}">${name}</option>`).join('')}</select><label for="bipProfileName">${tr('Nombre del perfil','Profile name')}</label><input id="bipProfileName" maxlength="100" required placeholder="${tr('Mi perfil personal','My personal profile')}"><label for="bipProfileHandle">${tr('Usuario o enlace del perfil','Username or profile link')}</label><input id="bipProfileHandle" maxlength="150" required placeholder="@usuario"><label for="bipProfileStyle">${tr('Estilo habitual','Usual style')}</label><select id="bipProfileStyle">${styles('linkedin').map(style=>`<option value="${style.id}">${esc(style[getLanguage()==='es'?'es':'en'])}</option>`).join('')}</select><button class="button" type="submit" ${controlsDisabled()}>${tr('Añadir perfil','Add profile')}</button></form></section>
    <section class="bip-connection"><h2>Codex ${tr('y otros asistentes','and other assistants')}</h2><p>${tr('Tu IA puede recoger ideas, perfiles y prompts, y devolver sus propuestas aquí. La publicación se gestiona después con tu asistente y sus herramientas.','Your AI can pick up ideas, profiles and prompts, then return proposals here. Publishing is handled later with your assistant and its tools.')}</p><button class="button" data-action="token" ${controlsDisabled()}>${tr('Crear conexión de asistente','Create assistant connection')}</button>${secret?`<div class="bip-token"><label for="bipToken">${tr('Clave de conexión · cópiala ahora','Connection key · copy it now')}</label><input id="bipToken" type="password" readonly value="${esc(secret)}"><button class="bip-text-button" data-action="copy-token">${tr('Copiar clave','Copy key')}</button><p class="bip-note">MCP URL</p><code>https://dzpyrdxelcgfpmcdojvb.supabase.co/functions/v1/build-in-public-assistant</code></div>`:''}${state?.tokens.filter(token=>token.scope==='assistant').map(token=>`<div class="bip-token-row"><span>${esc(token.prefix)}…</span><button class="bip-text-button" data-revoke="${token.id}">${tr('Revocar','Revoke')}</button></div>`).join('')||''}</section>
    <section class="bip-connection"><h2>${tr('Tu contexto','Your context')}</h2><form id="bipSettings"><label for="bipProject">${tr('Proyecto','Project')}</label><input id="bipProject" maxlength="100" required value="${esc(prefs.project_name)}"><label for="bipVoice">${tr('Cómo quieres contarlo','How you want to tell it')}</label><textarea id="bipVoice" maxlength="1500" rows="3" placeholder="${tr('Mi audiencia, mi tono, lo que quiero compartir…','My audience, tone, and what I want to share…')}">${esc(prefs.voice)}</textarea><label for="bipExcluded">${tr('Excluir palabras o proyectos','Exclude words or projects')}</label><input id="bipExcluded" value="${esc(prefs.excluded_terms.join(', '))}" placeholder="${tr('Separados por comas','Separated by commas')}"><label for="bipCadence">${tr('Buscar ideas automáticamente','Find ideas automatically')}</label><select id="bipCadence">${[['manual',tr('Solo cuando lo pida','Only when I ask')],['daily',tr('Cada día','Daily')],['weekly',tr('Cada semana','Weekly')]].map(([v,t])=>`<option value="${v}" ${prefs.cadence===v?'selected':''}>${t}</option>`).join('')}</select><p class="bip-note">${tr('Mientras FlowSight está abierto. Las ideas siguen en tu equipo hasta que las eliges.','While FlowSight is open. Ideas stay on your device until you choose them.')}</p><button type="submit" class="button" ${controlsDisabled()}>${tr('Guardar preferencias','Save preferences')}</button></form></section>`;}
  function evidenceEditor(){const value=editor.value;return `<button class="bip-back" data-action="back" ${controlsDisabled()}>← ${tr('Historias','Stories')}</button><div class="bip-section-heading"><h2>${tr('Revisa esta idea','Review this idea')}</h2><span class="bip-state">${!value.id?tr('Guardada en tu equipo','Saved on your device'):editor.dirty?tr('Cambios sin guardar','Unsaved changes'):tr('Guardada','Saved')}</span></div>${value.signals?.rationale?`<p class="bip-note">${esc(value.signals.rationale)}</p>`:''}${value.signals?.angle?`<p class="bip-note">${esc(value.signals.angle)} · ${esc(signalLine(value))}</p>`:''}<form id="bipEvidenceEdit"><label for="bipTitle">${tr('La idea','The idea')}</label><input id="bipTitle" maxlength="160" required value="${esc(value.title)}"><label for="bipSummary">${tr('Qué hiciste y qué cambió','What you did and what changed')}</label><textarea id="bipSummary" rows="7" maxlength="2000" required>${esc(value.summary)}</textarea><label class="bip-choice bip-confirm"><input id="bipConfirmed" type="checkbox" ${value.confirmed?'checked':''}><span>${tr('He revisado el texto y describe fielmente mi trabajo.','I reviewed the text and it accurately describes my work.')}</span></label><p class="bip-note">${tr('La observación puede contener errores. Corrige los hechos antes de guardar. Solo esta idea pasará a tu espacio privado online.','The observation may contain errors. Correct the facts before saving. Only this idea goes to your private online space.')}</p><button class="button button-primary bip-primary" type="submit" ${controlsDisabled()}>${tr('Guardar y seleccionar','Save and select')}</button></form>`;}
  function draftEditor(){const draft=editor.value;return `<button class="bip-back" data-action="back">← ${tr('Contenido','Content')}</button><div class="bip-section-heading"><h2>${esc(state.evidence.find(item=>item.id===draft.evidence_id)?.title||channelName[draft.channel])}</h2><span class="bip-state">${editor.dirty?tr('Cambios sin guardar','Unsaved changes'):tr('Propuesta de tu IA','AI proposal')}</span></div><form id="bipDraftEdit"><label for="bipBody">${channelName[draft.channel]} · ${tr('Texto de la propuesta','Proposed text')}</label><textarea id="bipBody" rows="9" required maxlength="5000">${esc(draft.body)}</textarea><p class="bip-counter" id="bipCounter">${contentLength(draft.body,draft.channel)} / ${limits[draft.channel]}</p><label for="bipDetails">${tr('Desarrollo, hilo o guion','Full thread, script or plan')}</label><textarea id="bipDetails" rows="7" maxlength="20000" placeholder="${tr('Tu IA puede dejar aquí el contenido completo.','Your AI can leave the full content here.')}">${esc(draft.details||'')}</textarea>${draft.visual?`<div class="bip-visual-editor"><h3>${tr('Boceto visual','Visual outline')}</h3><p class="bip-note">${tr('Es un punto de partida para tu IA creativa.','A starting point for your creative AI.')}</p><label for="bipVisualTitle">${tr('Título','Title')}</label><input id="bipVisualTitle" maxlength="100" value="${esc(draft.visual.title)}"><label for="bipVisualDetail">${tr('Idea visual','Visual idea')}</label><textarea id="bipVisualDetail" rows="3" maxlength="280">${esc(draft.visual.detail)}</textarea><label for="bipVisualFooter">${tr('Firma','Signature')}</label><input id="bipVisualFooter" maxlength="80" value="${esc(draft.visual.footer)}"><img class="bip-image" id="bipImage" alt="${tr('Boceto de imagen para Instagram','Instagram image outline')}"></div>`:''}<button type="submit" class="button bip-primary" ${controlsDisabled()}>${tr('Guardar cambios','Save changes')}</button></form><div class="bip-review-actions"><button class="button button-primary bip-primary" data-action="copy-draft">${tr('Copiar contenido','Copy content')}</button><p class="bip-note">${tr('Continúa con tu asistente para terminar el contenido y decidir cómo publicarlo.','Continue with your assistant to finish the content and decide how to publish it.')}</p><button class="bip-text-button" data-action="delete">${tr('Eliminar propuesta','Delete proposal')}</button></div>`;}
  function render(){
    const active=document.activeElement;
    if(root.contains(active)&&active!==root){
      const selector=active.id?`#${CSS.escape(active.id)}`:Object.entries(active.dataset).map(([key,value])=>`[data-${key.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())}="${CSS.escape(value)}"]`).join('');
      pendingFocus=selector?{selector,start:active.selectionStart,end:active.selectionEnd}:null;
    }
    if(!root.querySelector('.bip-status'))root.innerHTML='<div class="bip-header-slot"></div><div class="bip-status" role="status" aria-live="polite" aria-atomic="true"></div><div class="bip-content"></div>';
    root.querySelector('.bip-header-slot').innerHTML=header();
    const live=root.querySelector('.bip-status');live.setAttribute('role',error?'alert':'status');live.textContent=error||notice||(busy&&!collecting?tr('Preparando…','Preparing…'):'');
    root.querySelector('.bip-content').innerHTML=`${editor?(editor.type==='evidence'?evidenceEditor():editor.type==='brief'?briefEditor():draftEditor()):tab==='stories'?stories():tab==='prompts'?prompts():tab==='drafts'?drafts():connections()}${!loaded?`<p class="bip-note bip-connection-status" role="status">${connectionMessage()}</p><button class="bip-text-button" data-action="refresh" ${refreshing?'disabled':''}>${tr('Reintentar conexión','Retry connection')}</button>`:''}`;
    root.querySelectorAll('form button[type=submit]').forEach(button=>{button.id=`${button.form.id}Submit`;});
    for(const [id,values] of formDrafts){
      const form=root.querySelector(`#${id}`);if(!form)continue;
      if(id==='bipProfileForm'&&values.bipProfileNetwork)form.querySelector('#bipProfileStyle').innerHTML=styles(values.bipProfileNetwork).map(style=>`<option value="${style.id}">${esc(style[getLanguage()==='es'?'es':'en'])}</option>`).join('');
      for(const [field,value] of Object.entries(values)){const input=form.querySelector(`#${field}`);if(input)input.value=value;}
    }
    if(busy)root.querySelectorAll('form input,form textarea,form select').forEach(input=>{input.disabled=true;});
    if(editor?.type==='draft'&&editor.value.visual){const img=root.querySelector('#bipImage');if(img)img.src=drawContentImage(editor.value.visual);}
    if(pendingFocus&&!busy){
      const next=root.querySelector(pendingFocus.selector)||root.querySelector('.bip-back')||root.querySelector('.bip-content h2')||root.querySelector('h1');
      if(next){if(!next.matches('button,input,textarea,select,a'))next.tabIndex=-1;next.focus({preventScroll:true});if(pendingFocus.start!=null&&next.setSelectionRange)try{next.setSelectionRange(pendingFocus.start,pendingFocus.end);}catch{/* Not a text input. */}}
      pendingFocus=null;
    }
  }
  function markDraftDirty(){
    if(editor?.type!=='draft')return;
    captureEditor();
    const saved=state.drafts.find(item=>item.id===editor.value.id),value=editor.value;
    const dirty=Boolean(saved&&(value.body!==saved.body||value.details!==saved.details||value.provider_channel_id!==saved.provider_channel_id||value.scheduled_at!==saved.scheduled_at||['title','detail','footer'].some(key=>value.visual?.[key]!==saved.visual?.[key])));
    if(Boolean(editor.dirty)!==dirty){editor.dirty=dirty;render();}
  }
  function markEditorDirty(){
    if(!editor)return;
    if(editor.type==='draft'){markDraftDirty();return;}
    captureEditor();
    const saved=editor.type==='brief'?state.briefs.find(item=>item.id===editor.value.id):state.evidence.find(item=>item.id===editor.value.id);
    const fields=editor.type==='brief'?['prompt']:['title','summary','confirmed'];
    const dirty=!saved||fields.some(key=>editor.value[key]!==saved[key]);
    if(Boolean(editor.dirty)!==dirty){editor.dirty=dirty;render();}
    cacheEditor();
  }
  function captureEditor(){
    if(!editor)return;
    if(editor.type==='brief'){editor.value.prompt=root.querySelector('#bipPrompt')?.value??editor.value.prompt;return;}
    if(editor.type==='evidence'){editor.value={...editor.value,title:root.querySelector('#bipTitle')?.value??editor.value.title,summary:root.querySelector('#bipSummary')?.value??editor.value.summary,confirmed:root.querySelector('#bipConfirmed')?.checked??editor.value.confirmed};return;}
    const body=root.querySelector('#bipBody');if(!body)return;
    editor.value={...editor.value,body:body.value,details:root.querySelector('#bipDetails')?.value||'',provider_channel_id:null,scheduled_at:null};
    if(editor.value.visual&&root.querySelector('#bipVisualTitle'))editor.value.visual={title:root.querySelector('#bipVisualTitle').value,detail:root.querySelector('#bipVisualDetail').value,footer:root.querySelector('#bipVisualFooter').value};
  }
  function captureForm(form){
    if(!form||!['bipProfileForm','bipSettings','bipDirectionForm'].includes(form.id))return;
    formDrafts.set(form.id,Object.fromEntries([...form.querySelectorAll('input,textarea,select')].map(input=>[input.id,input.value])));
  }
  function cacheEditor(){
    if(!editor)return;
    editing.set(editorKey(editor.value),structuredClone(editor));
    if(editor.manual){manualDraft=structuredClone(editor);persistLocal();}
    if(editor.type==='evidence'&&editor.localKey){for(const items of [local,history]){const index=items.findIndex(item=>item.source_key===editor.localKey);if(index>=0)items[index]={...items[index],...editor.value};}persistLocal();}
  }
  function acceptIdeas(items){
    const keep=new Map([...local,...history].map(item=>[item.source_key,item]));
    for(const item of items)keep.delete(item.source_key);
    history=[...keep.values()];local=items;showHistory=false;
    recentKeys=[...new Set([...items.flatMap(item=>[item.source_key,...(item.origin_key?[`used:${item.origin_key}`]:[])]),...recentKeys])].slice(0,100);persistLocal();
  }
  async function collectIdeas(){
    if(busy)return;
    const direction=root.querySelector('#bipDirection')?.value??state?.settings.topic_direction??'';
    collecting=true;
    await run(async()=>{await localReady;saveLocalDirection(direction);const owner=state.owner_id;
      const recentTitles=[...new Set([...local,...history,...(state?.evidence||[])].map(item=>item.title).filter(Boolean))].slice(0,20);
      const result=await invoke('bip_collect_local',{periodDays:days,locale:getLanguage(),excludedTerms:state?.settings.excluded_terms||[],direction,recentKeys,recentTitles,includeCodexPrompts});
      if(owner!==state.owner_id)return;
      if(!Array.isArray(result.evidence))throw tr('La IA no completó la búsqueda. Tus ideas anteriores siguen guardadas.','The AI did not complete the search. Your previous ideas are still saved.');
      if(result.status==='no_new_ideas')notice=tr('No he encontrado ideas nuevas sin repetir las anteriores. Prueba otro enfoque o más días.','No new ideas found without repeating earlier ones. Try another direction or more days.');
      else {acceptIdeas(result.evidence);if(!result.evidence.length)notice=direction.trim()?tr('No he encontrado el tema solicitado en las fuentes de este periodo. Prueba más días o incluye tus prompts de Codex.','The requested topic was not found in the sources for this period. Try more days or include your Codex prompts.'):tr('No hay suficiente contexto en este periodo. Prueba más días o incluye tus prompts de Codex.','Not enough context in this period. Try more days or include your Codex prompts.');else if(result.evidence.length<5)notice=tr(`He encontrado ${result.evidence.length} ideas distintas con contexto suficiente.`,`Found ${result.evidence.length} distinct ideas with enough context.`);}
    },{reload:false});
    collecting=false;render();
  }
  root.addEventListener('toggle',event=>{if(event.target.matches?.('.bip-direction'))directionOpen=event.target.open;},true);
  async function saveDraft(){
    const draft=editor.value;
    await api({action:'edit',id:draft.id,revision:draft.revision,draft:{body:draft.body,details:draft.details||'',visual:draft.visual,provider_channel_id:draft.provider_channel_id,scheduled_at:draft.scheduled_at}});
    state=await api({action:'studio'});editor.value=state.drafts.find(item=>item.id===draft.id);
    editing.delete(draft.id);editor.dirty=false;
    if(draft.channel==='instagram'&&draft.visual&&!editor.value.image_path){await document.fonts.ready;const png=drawContentImage(draft.visual).split(',')[1];await api({action:'image',id:draft.id,revision:editor.value.revision,png});state=await api({action:'studio'});editor.value=state.drafts.find(item=>item.id===draft.id);}
  }
  root.addEventListener('click',event=>{
    const button=event.target.closest('button');if(!button||busy)return;
    captureEditor();
    cacheEditor();
    if(button.dataset.view){tab=button.dataset.view;editor=null;setup=false;error='';notice='';render();const heading=root.querySelector('.bip-content h2')||root.querySelector('h1');heading.tabIndex=-1;heading.focus({preventScroll:true});return;}
    if(button.dataset.local!==undefined){const item=localItems()[Number(button.dataset.local)];editor=editing.get(editorKey(item))||{type:'evidence',value:{...item,confirmed:item.confirmed||false},localKey:item.source_key};render();return;}
    if(button.dataset.evidence){editor=editing.get(button.dataset.evidence)||{type:'evidence',value:{...state.evidence.find(item=>item.id===button.dataset.evidence)}};render();return;}
    if(button.dataset.draft){editor=editing.get(button.dataset.draft)||{type:'draft',value:structuredClone(state.drafts.find(item=>item.id===button.dataset.draft))};render();return;}
    if(button.dataset.brief){editor=editing.get(button.dataset.brief)||{type:'brief',value:structuredClone(state.briefs.find(item=>item.id===button.dataset.brief))};render();return;}
    if(button.dataset.deleteProfile){run(async()=>{await api({action:'delete_profile',id:button.dataset.deleteProfile});for(const target of Object.values(targets))if(target.profile_id===button.dataset.deleteProfile)target.profile_id=null;});return;}
    if(button.dataset.revoke){run(()=>api({action:'revoke_token',id:button.dataset.revoke}));return;}
    const action=button.dataset.action;
    if(action==='back'){editor=null;render();return;}
    if(action==='history'){showHistory=!showHistory;render();return;}
    if(action==='setup'){setup=true;render();root.querySelector('.bip-back')?.focus();return;}
    if(action==='cancel-setup'){setup=false;render();return;}
    if(action==='manual'){editor=manualDraft||{type:'evidence',manual:true,value:{source_key:`manual:${crypto.randomUUID()}`,title:'',summary:'',occurred_on:new Date().toLocaleDateString('en-CA'),kind:'progress',confirmed:false}};render();return;}
    if(action==='refresh'){refresh();return;}
    if(action==='collect')collectIdeas();
    if(action==='generate')run(async()=>{await api({action:'briefs',request:{batch_key:crypto.randomUUID(),evidence_ids:selected().slice(0,3).map(item=>item.id),targets:networks.map(channel=>({channel,...targets[channel]}))}});tab='prompts';notice=tr('Encargos listos. Tu IA ya puede recogerlos.','Briefs ready. Your AI can pick them up now.');});
    if(action==='copy-brief')navigator.clipboard.writeText(editor.value.prompt).then(()=>{notice=tr('Prompt copiado. Pégalo en tu IA.','Prompt copied. Paste it into your AI.');render();}).catch(()=>{error=tr('Selecciona el texto del prompt para copiarlo.','Select the prompt text to copy it.');render();});
    if(action==='delete-brief')run(async()=>{await api({action:'delete_brief',id:editor.value.id});editing.delete(editor.value.id);editor=null;});
    if(action==='delete')run(async()=>{await api({action:'delete',id:editor.value.id});editing.delete(editor.value.id);editor=null;});
    if(action==='token')run(async()=>{secret=(await api({action:'assistant_token'})).token;});
    if(action==='copy-token')navigator.clipboard.writeText(secret).then(()=>{notice=tr('Clave copiada.','Key copied.');render();}).catch(()=>{error=tr('No se pudo copiar.','Could not copy.');render();});
    if(action==='copy-draft')navigator.clipboard.writeText([editor.value.body,editor.value.details].filter(Boolean).join('\n\n')).then(()=>{notice=tr('Texto copiado.','Text copied.');render();}).catch(()=>{error=tr('No se pudo copiar.','Could not copy.');render();});
  });
  root.addEventListener('change',event=>{
    if(event.target.dataset.style)targets[event.target.dataset.style].style=event.target.value;
    if(event.target.dataset.profile)targets[event.target.dataset.profile].profile_id=event.target.value||null;
    if(event.target.id==='bipProfileNetwork')root.querySelector('#bipProfileStyle').innerHTML=styles(event.target.value).map(style=>`<option value="${style.id}">${esc(style[getLanguage()==='es'?'es':'en'])}</option>`).join('');
    captureForm(event.target.closest('form'));
    if(event.target.id==='bipConfirmed')markEditorDirty();
    if(event.target.id==='bipPeriod'){days=Number(event.target.value);persistLocal();}
    if(event.target.id==='bipCodexPrompts'){includeCodexPrompts=event.target.checked;persistLocal();}
    if(event.target.dataset.network){networks=event.target.checked?[...new Set([...networks,event.target.dataset.network])]:networks.filter(item=>item!==event.target.dataset.network);render();}
    if(event.target.dataset.select)run(()=>api({action:'evidence',id:event.target.dataset.select,patch:{state:event.target.checked?'selected':'suggested'}}));
  });
  root.addEventListener('input',event=>{captureForm(event.target.closest('form'));if(event.target.closest('#bipEvidenceEdit,#bipBriefEdit,#bipDraftEdit'))markEditorDirty();if(event.target.id==='bipBody')root.querySelector('#bipCounter').textContent=`${contentLength(event.target.value,editor.value.channel)} / ${limits[editor.value.channel]}`;if(event.target.id?.startsWith('bipVisual')){captureEditor();root.querySelector('#bipImage').src=drawContentImage(editor.value.visual);}});
  root.addEventListener('submit',event=>{
    event.preventDefault();if(busy)return;
    const form=event.target;if(!form.reportValidity())return;
    captureForm(form);
    if(form.id==='bipDirectionForm'){const text=root.querySelector('#bipDirection').value;run(async()=>{await localReady;saveLocalDirection(text);if(loaded)await syncDirection(state);notice=tr('Enfoque guardado en tu equipo.','Direction saved on your device.');},{reload:false});}
    if(form.id==='bipProfileForm'){const profile={channel:root.querySelector('#bipProfileNetwork').value,name:root.querySelector('#bipProfileName').value,handle:root.querySelector('#bipProfileHandle').value,style:root.querySelector('#bipProfileStyle').value};run(async()=>{await api({action:'profile',profile});formDrafts.delete(form.id);notice=tr('Perfil añadido a FlowSight.','Profile added to FlowSight.');});}
    if(form.id==='bipBriefEdit'){captureEditor();run(async()=>{const id=editor.value.id;await api({action:'edit_brief',id,revision:editor.value.revision,prompt:editor.value.prompt});state=await api({action:'studio'});editor.value=state.briefs.find(item=>item.id===id);editor.dirty=false;editing.delete(id);notice=tr('Prompt guardado.','Prompt saved.');});}
    if(form.id==='bipSettings'){const prefs={project_name:root.querySelector('#bipProject').value,voice:root.querySelector('#bipVoice').value,excluded_terms:root.querySelector('#bipExcluded').value.split(',').map(x=>x.trim()).filter(Boolean),cadence:root.querySelector('#bipCadence').value,locale:getLanguage()==='es'?'es':'en',timezone:Intl.DateTimeFormat().resolvedOptions().timeZone};run(async()=>{await api({action:'settings',settings:prefs});formDrafts.delete(form.id);notice=tr('Preferencias guardadas.','Preferences saved.');});}
    if(form.id==='bipDraftEdit'){captureEditor();run(async()=>{await saveDraft();notice=tr('Cambios guardados.','Changes saved.');});}
    if(form.id==='bipEvidenceEdit'){captureEditor();const item=editor.value;run(async()=>{if(!item.id){const {source_key,title,summary,occurred_on,kind,signals={}}=item;await api({action:'import',evidence:[{source_key,title,summary,occurred_on,kind,signals}],manual:editor.manual===true});state=await api({action:'studio'});item.id=state.evidence.find(row=>row.source_key===item.source_key)?.id;}await api({action:'evidence',id:item.id,patch:{title:item.title,summary:item.summary,confirmed:item.confirmed,state:'selected'}});editing.delete(editorKey(item));if(editor.manual){manualDraft=null;persistLocal();}if(editor.localKey){editing.delete(`local:${editor.localKey}`);local=local.filter(row=>row.source_key!==editor.localKey);history=history.filter(row=>row.source_key!==editor.localKey);persistLocal();}editor=null;});}
  });
  async function checkCadence(){
    if(!canUse()||busy||refreshing||editor)return;
    if(root.contains(document.activeElement)&&document.activeElement.closest('form'))return;
    const before=JSON.stringify([state?.owner_id,state?.briefs,state?.drafts,state?.settings.topic_direction]);
    refreshing=true;
    try{await localReady;await resolveLocalOwner();const next=await api({action:'studio'});if(busy||editor)return;await syncDirection(next);if(busy||editor)return;acceptStudio(next);}catch{loaded=false;secret='';}finally{refreshing=false;}
    if(before!==JSON.stringify([state.owner_id,state.briefs,state.drafts,state.settings.topic_direction])&&!root.hidden){notice=tr('Tu contenido está actualizado.','Your content is up to date.');render();}
    if(state.settings.cadence==='manual')return;
    const prefs=state.settings,key=`bip-ideas-${state.owner_id||'local'}`,previous=Number(localStorage.getItem(key)||0),interval=prefs.cadence==='daily'?86400000:7*86400000;
    if(Date.now()-previous<interval)return;
    try{busy=true;collecting=true;render();const recentTitles=[...new Set([...local,...history,...(state?.evidence||[])].map(item=>item.title).filter(Boolean))].slice(0,20);const result=await invoke('bip_collect_local',{periodDays:prefs.cadence==='daily'?1:7,locale:getLanguage(),excludedTerms:prefs.excluded_terms,direction:prefs.topic_direction||'',recentKeys,recentTitles,includeCodexPrompts});if(result.evidence.length){acceptIdeas(result.evidence);localStorage.setItem(key,String(Date.now()));}}catch{/* An offline suggestion must not interrupt focus. */}finally{busy=false;collecting=false;if(!root.hidden)render();}
  }
  document.addEventListener('flowsight:languagechange',()=>{captureEditor();render();});
  const localReady=resolveLocalOwner().catch(()=>{if(localOwner===null)acceptStudio(emptyStudio('device'),{online:false});}).then(()=>{if(!disposed&&!busy)render();});
  cadenceTimer=setInterval(checkCadence,60000);
  render();
  return {refresh,dispose(){disposed=true;clearInterval(cadenceTimer);secret='';}};
}
