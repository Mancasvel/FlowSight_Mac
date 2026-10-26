// Actual compiled app/API/database/MCP; only the activity fixture is synthetic.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const {chromium}=await import(process.env.FLOWSIGHT_PLAYWRIGHT_MODULE||'playwright');
const token=process.env.FLOWSIGHT_BIP_TEST_TOKEN;
if(!token)throw new Error('A disposable test-user token is required.');
const userId=JSON.parse(Buffer.from(token.split('.')[1],'base64url').toString()).sub;
const endpoint='https://dzpyrdxelcgfpmcdojvb.supabase.co/functions/v1/';
async function api(body){const response=await fetch(endpoint+'build-in-public',{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify(body)});const value=await response.json();assert.ok(response.ok,JSON.stringify(value));return value;}
const source=await readFile(new URL('./verify-recorded-time.mjs',import.meta.url),'utf8');
const begin=source.indexOf('await page.addInitScript(')+'await page.addInitScript('.length;
const end=source.indexOf('}, { locale });',begin)+1;
const output=resolve(process.env.FLOWSIGHT_BIP_EVIDENCE||'.impeccable/review/build-in-public');await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});
try{
 const page=await browser.newPage({locale:'es-ES',timezoneId:'Europe/Madrid',viewport:{width:370,height:700},reducedMotion:'reduce'});
 const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.addInitScript({content:`(${source.slice(begin,end)})({locale:'es-ES'});`});
 await page.addInitScript(({token,endpoint,userId})=>{
  window.__bipTest={failNext:'',poll:null,batch:0,lastCollection:null,mode:''};
  const interval=window.setInterval.bind(window);
  window.setInterval=(fn,delay,...args)=>{if(delay===60000&&fn.toString().includes('bip_collect_local'))window.__bipTest.poll=fn;return interval(fn,delay,...args);};
  const original=window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
   if(command==='get_current_user')return {user_id:userId,email:'fixture@example.invalid'};
   if(command==='bip_cloud'&&window.__bipTest.failNext===args.request.action){window.__bipTest.failNext='';throw 'Fallo de conexión simulado para comprobar la recuperación.';}
   if(command==='bip_cloud'){const response=await fetch(endpoint+'build-in-public',{method:'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},body:JSON.stringify(args.request)});const payload=await response.json();if(!response.ok)throw payload.error;return payload;}
   if(command==='bip_collect_local'){
    window.__bipTest.lastCollection=args;
    if(window.__bipTest.mode==='missing')return {evidence:[],status:'no_matches'};
    if(window.__bipTest.mode==='exhausted')return {evidence:[],status:'no_new_ideas'};
    if(window.__bipTest.failNext==='collect'){window.__bipTest.failNext='';throw 'Fallo de conexión local simulado. Tus ideas anteriores siguen aquí.';}
    const batch=++window.__bipTest.batch;
    const titles=['La música acompaña a las pausas de trabajo','Elegir dónde empieza una sesión de concentración','Qué revisar antes de entregar una mejora','Una interfaz que deja sitio al trabajo','Cómo explicar una decisión de producto'];
    const facts=['Revisando la pausa de YouTube al pausar una sesión.','Comparando el inicio de Total focus desde Today y Ajustes.','Probando la recuperación de una sesión tras reiniciar la app.','Ajustando la jerarquía de los controles de concentración.','Preparando una explicación visual del recorrido de inicio.'];
    return {evidence:titles.slice(0,window.__bipTest.mode==='partial'?2:5).map((title,i)=>({source_key:`synthetic-${batch}-${i}`,title:batch===1?title:`Otro enfoque: ${title}`,summary:`${facts[i]} Ejemplo sintético para la prueba.`,occurred_on:'2026-10-12',kind:'progress',signals:{observations:12-i,recorded_seconds:3600-i*200,days:2,topic:title,rationale:'Un detalle de producto que puede servir a otros creadores.',angle:['El problema','Una decisión','El proceso','Un detalle','Una pregunta'][i]}}))};
   }
   if(command==='bip_generate_local')throw new Error('Prompt preparation must not invoke local copywriting.');
   return original(command,args);
  };
 },{token,endpoint,userId});
 const click=selector=>page.locator(selector).click();
 const status=text=>page.locator('.bip-status').filter({hasText:text}).waitFor();
 const focus=async id=>assert.equal(await page.evaluate(()=>document.activeElement.id),id);
 await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1443',{waitUntil:'networkidle'});
 await click('#navOnline');await click('#onlineContentTab');await click('[data-view=connections]');
 await page.locator('#bipProfileNetwork').selectOption('x');await page.locator('#bipProfileName').fill('Perfil de prueba');await page.locator('#bipProfileHandle').fill('@synthetic_builder');await page.locator('#bipProfileStyle').selectOption('thread');
 await page.evaluate(()=>{window.__bipTest.failNext='profile';});await click('#bipProfileForm button[type=submit]');await status('Fallo de conexión');
 assert.equal(await page.locator('#bipProfileName').inputValue(),'Perfil de prueba');assert.equal(await page.locator('#bipProfileHandle').inputValue(),'@synthetic_builder');assert.equal(await page.locator('#bipProfileNetwork').inputValue(),'x');assert.equal(await page.locator('#bipProfileStyle').inputValue(),'thread');
 await click('#bipProfileForm button[type=submit]');await status('Perfil añadido');await focus('bipProfileFormSubmit');assert.equal(await page.locator('#bipBufferKey').count(),0);
 await page.locator('#bipVoice').fill('Contexto sintético que debe sobrevivir a un error de conexión.');await page.locator('#bipProject').fill('Prueba de persistencia');await page.locator('#bipExcluded').fill('privado');
 await page.evaluate(()=>{window.__bipTest.failNext='settings';});await click('#bipSettings button[type=submit]');await status('Fallo de conexión');
 assert.equal(await page.locator('#bipVoice').inputValue(),'Contexto sintético que debe sobrevivir a un error de conexión.');assert.equal(await page.locator('#bipProject').inputValue(),'Prueba de persistencia');assert.equal(await page.locator('#bipExcluded').inputValue(),'privado');
 await click('#bipSettings button[type=submit]');await status('Preferencias guardadas');
 await click('[data-view=stories]');await click('.bip-direction summary');await page.locator('#bipDirection').fill('Decisiones de producto para otros founders');await click('[data-action=collect]');await page.locator('[data-local="4"]').waitFor();
 assert.equal(await page.locator('[data-local]').count(),5);assert.equal(await page.evaluate(()=>window.__bipTest.lastCollection.direction),'Decisiones de producto para otros founders');
 await page.locator('#bipCodexPrompts').check();
 await click('.bip-direction summary');
 await click('[data-local="0"]');await page.locator('#bipTitle').fill('Mi edición local conservada');await click('[data-action=back]');await click('[data-local="0"]');assert.equal(await page.locator('#bipTitle').inputValue(),'Mi edición local conservada');
 await click('[data-action=back]');await page.evaluate(()=>{window.__bipTest.failNext='collect';});await click('[data-action=collect]');await status('Fallo de conexión local');assert.equal(await page.locator('[data-local]').count(),5);
 await click('[data-action=collect]');await page.locator('[data-local="0"] strong').filter({hasText:'Otro enfoque'}).waitFor();assert.equal(await page.locator('[data-local]').count(),5);assert.equal((await page.evaluate(()=>window.__bipTest.lastCollection.recentKeys)).length,5);
 assert.equal(await page.evaluate(()=>window.__bipTest.lastCollection.includeCodexPrompts),true);assert.ok((await page.evaluate(()=>window.__bipTest.lastCollection.recentTitles)).includes('Mi edición local conservada'));
 await click('[data-action=history]');await click('[data-local="0"]');assert.equal(await page.locator('#bipTitle').inputValue(),'Mi edición local conservada');
 await page.locator('#bipConfirmed').check();await click('#bipEvidenceEdit button[type=submit]');await page.locator('[data-evidence]').first().waitFor();
 await page.locator('[data-evidence]').first().click();await page.locator('#bipTitle').fill('Edición online pendiente');await click('[data-action=back]');await page.locator('[data-evidence]').first().click();assert.equal(await page.locator('#bipTitle').inputValue(),'Edición online pendiente');await click('[data-action=back]');
 await click('[data-action=history]');await page.locator('[data-action=setup]').first().click();await page.locator('[data-action=generate]').waitFor();
 await page.locator('[data-network=x]').uncheck();assert.equal(await page.evaluate(()=>document.activeElement.dataset.network),'x');await page.locator('[data-network=x]').check();await page.locator('#bipStyle-x').selectOption('thread');
 const state=await api({action:'studio'});await page.locator('#bipProfile-x').selectOption(state.profiles[0].id);
 await click('[data-action=generate]');await page.locator('[data-brief]').first().waitFor();assert.equal(await page.locator('[data-brief]').count(),3);
 await page.locator('[data-brief]').filter({hasText:'Hilo del proceso'}).click();const prompt=await page.locator('#bipPrompt').inputValue();assert.ok(prompt.includes('Hilo de 3 a 5'));assert.ok(prompt.includes('@synthetic_builder'));assert.ok(prompt.includes('no publica'));
 await page.locator('#bipPrompt').fill(prompt+'\n\nPreferencia sintética: evita tecnicismos.');await click('#bipBriefEdit button[type=submit]');await status('Prompt guardado');await focus('bipBriefEditSubmit');
 const assistant=(await api({action:'assistant_token'})).token;
 async function mcp(name,args={}){const response=await fetch(endpoint+'build-in-public-assistant',{method:'POST',headers:{Authorization:`Bearer ${assistant}`,'Content-Type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name,arguments:args}})});const value=await response.json();assert.ok(response.ok&&!value.result.isError,JSON.stringify(value));return value.result;}
 const context=JSON.parse((await mcp('get_content_context')).content[0].text);assert.equal(context.briefs.length,3);assert.equal(context.profiles.length,1);
 await mcp('set_content_direction',{text:'Enfoque cambiado desde Codex'});await click('[data-view=stories]');await page.evaluate(()=>window.__bipTest.poll());assert.equal(await page.locator('#bipDirection').inputValue(),'Enfoque cambiado desde Codex');
 const drafts=context.briefs.map(brief=>({brief_id:brief.id,evidence_id:brief.evidence_id,channel:brief.channel,body:'Propuesta sintética: la música acompaña a la pausa de concentración.',details:brief.channel==='x'?'1. Problema observado.\n2. Decisión de diseño.\n3. Próximo paso.':'Desarrollo sintético devuelto por el asistente.',visual:brief.channel==='instagram'?{title:'La pausa también se escucha',detail:'Un boceto sintético para explicar el trabajo.',footer:'FlowSight · Prueba'}:null}));
 await click('[data-view=drafts]');assert.equal(await page.locator('[data-draft]').count(),0);
 await mcp('save_content_drafts',{batch_key:crypto.randomUUID(),drafts});
 await page.evaluate(()=>window.__bipTest.poll());await page.locator('[data-draft]').first().waitFor();assert.equal(await page.locator('[data-draft]').count(),3);assert.equal(await page.locator('[data-action=send],[data-action=approve]').count(),0);
 await page.locator('[data-draft]').filter({hasText:'Instagram'}).click();await page.locator('#bipBody').fill('Texto sintético revisado.');await click('#bipDraftEdit button[type=submit]');await status('Cambios guardados');await focus('bipDraftEditSubmit');
 await click('[data-view=stories]');await click('[data-action=manual]');await page.locator('#bipTitle').fill('Un hito manual pendiente');await page.locator('#bipSummary').fill('Un hecho sintético que todavía no he guardado online.');await click('[data-action=back]');await click('[data-action=manual]');assert.equal(await page.locator('#bipTitle').inputValue(),'Un hito manual pendiente');await click('[data-view=prompts]');await click('[data-view=stories]');await click('[data-action=manual]');assert.equal(await page.locator('#bipSummary').inputValue(),'Un hecho sintético que todavía no he guardado online.');assert.equal((await api({action:'studio'})).evidence.length,1);
 const layout=[];
 for(const [name,width,height,scheme] of [['compact',370,700,'light'],['minimum',340,400,'light'],['wide',900,780,'light'],['dark',370,700,'dark']]){
  await page.setViewportSize({width,height});await page.emulateMedia({colorScheme:scheme});await page.waitForTimeout(250);
  for(const view of ['stories','prompts','drafts','connections']){
   await click(`[data-view=${view}]`);await page.evaluate(()=>{window.scrollTo(0,0);document.querySelectorAll('*').forEach(el=>{if(el.scrollTop)el.scrollTop=0;});});
   assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.screenshot({path:resolve(output,`${name}-${view}.png`)});
   if(view==='stories')layout.push({name,rows:await page.locator('[data-local] strong').evaluateAll(items=>items.map(item=>({y:item.getBoundingClientRect().y,height:item.getBoundingClientRect().height}))) });
  }
  await click('[data-view=stories]');await page.locator('[data-action=setup]').first().click();await page.screenshot({path:resolve(output,`${name}-setup.png`)});
  await click('[data-action=cancel-setup]');await click('[data-local="0"]');await page.screenshot({path:resolve(output,`${name}-idea-editor.png`)});
  await click('[data-view=prompts]');await page.locator('[data-brief]').first().click();await page.screenshot({path:resolve(output,`${name}-prompt-editor.png`)});
 }
 await writeFile(resolve(output,'layout.json'),JSON.stringify(layout,null,2));
 await click('[data-view=stories]');
 await page.evaluate(()=>{window.__bipTest.mode='exhausted';});await click('[data-action=collect]');await status('No he encontrado ideas nuevas');assert.equal(await page.locator('[data-local]').count(),5);
 await page.evaluate(()=>{window.__bipTest.mode='missing';});await click('[data-action=collect]');await status('No he encontrado el tema solicitado');assert.equal(await page.locator('[data-local]').count(),0);assert.equal(await page.locator('[data-action=history]').count(),1);
 await page.evaluate(()=>{window.__bipTest.mode='partial';});await click('[data-action=collect]');await status('He encontrado 2 ideas');assert.equal(await page.locator('[data-local]').count(),2);
 assert.deepEqual(errors,[]);
 await writeFile(resolve(output,'result.json'),JSON.stringify({passed:true,checks:['Independent X profile without Buffer','Profile and preferences preserved after request failures','Per-network styles','Five distinct ideas per batch with guided direction and prior-key avoidance','Local and saved edits preserved across back, tabs and regeneration','Failed collection keeps previous five ideas','MCP topic direction reaches the app','Local ideas reviewed','Three prompts persisted','Prompt editing','MCP receives profiles and briefs','MCP returns full thread/visual content','Incoming MCP content appears without reloading in manual cadence','Returned content editable','No publishing controls','Focus after profile/prompt/content save','370x700,340x400,900x780 and dark'],data:'Synthetic disposable account; actual compiled renderer, live API/database and MCP'},null,2));
 console.log('Assistant-first Build in public: full renderer/API/database/MCP round-trip passed. Synthetic data.');
}finally{await browser.close();}
