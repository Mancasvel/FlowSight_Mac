// Actual compiled renderer with isolated native IPC and fictional local work.
import assert from 'node:assert/strict';
import {readFile,mkdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const {chromium}=await import(process.env.FLOWSIGHT_PLAYWRIGHT_MODULE||'playwright');
const source=await readFile(new URL('./verify-recorded-time.mjs',import.meta.url),'utf8');
const begin=source.indexOf('await page.addInitScript(')+'await page.addInitScript('.length;
const end=source.indexOf('}, { locale });',begin)+1;
const output=resolve(process.env.FLOWSIGHT_BIP_EVIDENCE||'.impeccable/review/build-in-public-offline');
await mkdir(output,{recursive:true});
const browser=await chromium.launch({headless:true});
const checks=[];
try{
  const page=await browser.newPage({locale:'es-ES',timezoneId:'Europe/Madrid',viewport:{width:390,height:800},reducedMotion:'reduce'});
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.addInitScript({content:`(${source.slice(begin,end)})({locale:'es-ES'});`});
  await page.addInitScript(()=>{
    const owner=()=>sessionStorage.getItem('bip-test-owner')||'offline-owner-a';
    window.__bipOffline={mode:'expired',calls:[],collection:null,poll:null};
    const interval=window.setInterval.bind(window);
    window.setInterval=(fn,delay,...args)=>{if(delay===60000&&fn.toString().includes('bip_collect_local'))window.__bipOffline.poll=fn;return interval(fn,delay,...args);};
    const original=window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
      if(command==='get_current_user')return {user_id:owner(),email:'fixture@example.invalid'};
      if(command==='get_entitlements'||command==='refresh_entitlements')return {owner_user_id:owner(),plan:'individual',status:'active',can_sync:false,can_cloud_ai:false,can_integrations:false,team_ids:[],usage_profile:{id:'individual',verified:true,license_plan_id:'solo',team_id:null,teams:[],available_profiles:['individual'],capabilities:{online:true,focus_alerts:true,personal_insights:true,team_dashboard:false}}};
      if(command==='bip_cloud'){
        window.__bipOffline.calls.push(args.request);
        if(window.__bipOffline.mode==='pending')return new Promise(()=>{});
        if(window.__bipOffline.mode!=='online')throw 'Your session is invalid or expired.';
        if(args.request.action==='studio')return {owner_id:owner(),settings:{project_name:'Fixture',voice:'',excluded_terms:['Private fixture'],cadence:'manual',topic_direction:'Server direction'},evidence:[],briefs:[],drafts:[],profiles:[],styles:{},tokens:[]};
        return {};
      }
      if(command==='bip_collect_local'){
        window.__bipOffline.collection=args;
        if(window.__bipOffline.failCollection)throw 'The local AI is offline';
        const batch=Number(sessionStorage.getItem('bip-test-batch')||0)+1;
        sessionStorage.setItem('bip-test-batch',String(batch));
        return {evidence:[{source_key:`fixture-${owner()}-${batch}`,title:`Decisión local ${batch}`,summary:'Revisando una decisión de producto. Fuente ficticia de la prueba.',occurred_on:'2026-10-12',kind:'progress',signals:{observations:2,recorded_seconds:120,days:1}}]};
      }
      return original(command,args);
    };
    if(!sessionStorage.getItem('bip-test-seeded')){
      // Existing users have this older cache shape, without settings or direction.
      localStorage.setItem('bip-local-ideas-offline-owner-a',JSON.stringify({evidence:[{source_key:'legacy',title:'Idea anterior',summary:'Material local anterior, conservado al actualizar la aplicación.',occurred_on:'2026-10-12',kind:'progress'}],history:[],recentKeys:['legacy'],includeCodexPrompts:true,at:Date.now()}));
      sessionStorage.setItem('bip-test-seeded','yes');
    }
  });
  async function openStudio(){
    await page.locator('#navOnline').click();await page.locator('#onlineContentTab').click();
    await page.locator('#bipPeriod').waitFor();
    await page.waitForFunction(()=>window.__bipOffline.calls.some(call=>call.action==='studio'));
  }
  async function waitCollection(){await page.waitForFunction(()=>window.__bipOffline.collection&&!document.querySelector('[data-action=collect]').disabled);}
  const cache=()=>page.evaluate(()=>JSON.parse(localStorage.getItem(`bip-local-ideas-${sessionStorage.getItem('bip-test-owner')||'offline-owner-a'}`)));
  await page.goto(process.env.FLOWSIGHT_RENDERER_URL||'http://127.0.0.1:1420');
  await openStudio();
  await page.locator('[data-local="0"]').filter({hasText:'Idea anterior'}).waitFor();
  assert.equal(await page.locator('#bipCodexPrompts').isChecked(),true);
  assert.equal(await page.locator('.bip-status').textContent(),'');
  assert.equal(await page.locator('[data-action=collect]').isEnabled(),true);
  checks.push('Expired session restores the existing owner cache and leaves local discovery available');

  await page.locator('.bip-direction summary').click();
  await page.locator('#bipDirection').fill('Mi trabajo con ESP32');
  await page.locator('#bipPeriod').selectOption('30');
  const cloudBefore=await page.evaluate(()=>window.__bipOffline.calls.length);
  await page.locator('[data-action=collect]').click();await waitCollection();
  assert.equal(await page.evaluate(()=>window.__bipOffline.calls.length),cloudBefore,'Finding ideas must make no cloud request');
  assert.deepEqual(await page.evaluate(()=>[window.__bipOffline.collection.direction,window.__bipOffline.collection.periodDays,window.__bipOffline.collection.includeCodexPrompts]),['Mi trabajo con ESP32',30,true]);
  assert.equal((await cache()).settings.topic_direction,'Mi trabajo con ESP32');
  assert.equal((await cache()).history[0].source_key,'legacy');
  checks.push('Local discovery uses the entered direction, period and prompt choice without calling the cloud');

  await page.locator('[data-local="0"]').click();await page.locator('#bipTitle').fill('Decisión corregida por mí');
  await page.locator('#bipEvidenceEdit button[type=submit]').click();
  await page.locator('.bip-status[role=alert]').waitFor();
  assert.equal(await page.locator('#bipTitle').inputValue(),'Decisión corregida por mí');
  assert.ok(!(await page.locator('.bip-status').textContent()).includes('Your session'));
  await page.locator('[data-action=back]').click();
  await page.reload();await openStudio();
  await page.locator('[data-local="0"]').filter({hasText:'Decisión corregida por mí'}).waitFor();
  assert.equal(await page.locator('#bipDirection').inputValue(),'Mi trabajo con ESP32');
  assert.equal(await page.locator('#bipPeriod').inputValue(),'30');
  assert.equal(await page.locator('#bipCodexPrompts').isChecked(),true);
  checks.push('Offline edits, unsent direction, period, history and Codex prompt preference survive reopening');

  await page.locator('.bip-direction summary').click();await page.locator('#bipDirection').fill('Otra decisión local');
  await page.locator('#bipDirectionForm button[type=submit]').click();
  await page.locator('.bip-status').filter({hasText:'Enfoque guardado en tu equipo'}).waitFor();
  assert.equal((await cache()).pendingDirection,'Otra decisión local');
  await page.evaluate(()=>{window.__bipOffline.failCollection=true;});
  await page.locator('[data-action=collect]').click();
  await page.locator('.bip-status[role=alert]').filter({hasText:'The local AI is offline'}).waitFor();
  assert.equal((await cache()).evidence[0].title,'Decisión corregida por mí');
  checks.push('Saving direction works offline and a real local-AI error preserves earlier ideas');

  await page.evaluate(()=>{window.__bipOffline.mode='pending';window.__bipOffline.failCollection=false;window.__bipOffline.collection=null;});
  await page.locator('[data-action=refresh]').click();
  await page.waitForFunction(()=>document.querySelector('[data-action=refresh]').disabled);
  assert.equal(await page.locator('[data-action=collect]').isEnabled(),true);
  await page.locator('[data-action=collect]').click();await waitCollection();
  assert.equal((await cache()).evidence[0].title,'Decisión local 2');
  checks.push('A cloud request that never resolves cannot block local generation');

  await page.reload();await openStudio();
  await page.evaluate(()=>{window.__bipOffline.mode='online';});
  await page.locator('[data-action=refresh]').click();
  await page.waitForFunction(()=>!document.querySelector('.bip-connection-status'));
  assert.equal(await page.locator('#bipDirection').inputValue(),'Otra decisión local');
  assert.equal((await cache()).pendingDirection,null);
  assert.ok(await page.evaluate(()=>window.__bipOffline.calls.some(call=>call.action==='direction'&&call.text==='Otra decisión local'&&call.expected_owner==='offline-owner-a')));
  assert.ok(await page.evaluate(()=>window.__bipOffline.calls.every(call=>!['import','evidence','briefs'].includes(call.action))));
  checks.push('Reconnection syncs only the pending direction to its owner and does not upload local evidence');
  await page.locator('.bip-direction summary').click();
  await page.locator('#bipDirection').fill('Búsqueda con exclusiones privadas');
  await page.locator('[data-action=collect]').click();await waitCollection();
  assert.deepEqual(await page.evaluate(()=>window.__bipOffline.collection.excludedTerms),['Private fixture']);

  await page.evaluate(()=>{sessionStorage.setItem('bip-test-owner','offline-owner-b');window.__bipOffline.mode='expired';});
  await page.reload();await openStudio();
  assert.equal(await page.locator('[data-local]').count(),0);
  assert.equal(await page.locator('#bipDirection').inputValue(),'');
  assert.equal(await page.locator('#bipCodexPrompts').isChecked(),false);
  assert.ok((await page.evaluate(()=>localStorage.getItem('bip-local-ideas-offline-owner-a'))).includes('Decisión corregida por mí'));
  checks.push('Switching accounts cannot reveal another owner’s local ideas or preferences');
  assert.deepEqual(errors,[]);
  await page.screenshot({path:resolve(output,'offline.png')});
  await writeFile(resolve(output,'result.json'),JSON.stringify({passed:true,checks,data:'Synthetic accounts/work and native IPC; actual compiled app renderer'},null,2)+'\n');
  console.log(JSON.stringify({passed:true,checks},null,2));
}finally{await browser.close();}
