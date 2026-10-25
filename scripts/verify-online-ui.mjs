// Real renderer; simulated native service solely for isolated UI verification.
import assert from 'node:assert/strict';
import {readFile,mkdir} from 'node:fs/promises';
import {chromium} from 'playwright';
const source=await readFile(new URL('./verify-recorded-time.mjs',import.meta.url),'utf8');
const begin=source.indexOf('await page.addInitScript(')+'await page.addInitScript('.length;
const end=source.indexOf('}, { locale });',begin)+1;
const initialize=source.slice(begin,end);
const out=new URL('../.impeccable/review/online/',import.meta.url);await mkdir(out,{recursive:true});
const browser=await chromium.launch({headless:true});
try{
  const page=await browser.newPage({locale:'es-ES',viewport:{width:370,height:700}});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript({content:`(${initialize})({locale:'es-ES'});`});
  await page.addInitScript(()=>{
    const original=window.__TAURI_INTERNALS__.invoke;
    window.onlineTestState={signed_in:true,enabled:false,groups:[],alias:''};
    window.onlineFail=false;window.usernameTaken=false;window.onlineHold=false;window.onlineCloudError='';
    window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
      if(!['get_online_leagues','set_online_league_consent','online_league_action'].includes(command))return original(command,args);
      window.testCalls.push({command,args});
      if(window.onlineFail)throw 'Online leagues could not connect. Check your connection and retry';
      if(command==='get_online_leagues'){
        const result=structuredClone(window.onlineTestState);
        if(window.onlineCloudError)Object.assign(result,{cloud_error:window.onlineCloudError,groups:[],today_points:undefined,week_start:undefined});
        if(window.onlineHold)return new Promise((resolve,reject)=>{window.finishOnlineRefresh=()=>resolve(result);window.failOnlineRefresh=()=>reject('Online leagues could not connect. Check your connection and retry');});
        return result;
      }
      if(window.usernameTaken&&command==='set_online_league_consent')throw 'Username already taken. Choose another';
      if(command==='set_online_league_consent'){
        Object.assign(window.onlineTestState,{enabled:args.accept,alias:args.alias,synced_at:args.accept?'2026-10-05T10:00:00Z':null,today_points:0,eligible_minutes:0,week_start:'2026-10-05',withdrawal_pending:false});
        if(!args.accept)window.onlineTestState.groups=[];
      }else if(args.request.action==='create'){
        window.onlineTestState.groups=[{id:'test-group',name:args.request.name,owner:true,members:[{alias:window.onlineTestState.alias,mine:true,points:0,rank:1}]}];
        window.onlineTestState.invitation_code='a'.repeat(64);
      }
      return structuredClone(window.onlineTestState);
    };
  });
  await page.goto('http://127.0.0.1:1420',{waitUntil:'networkidle'});
  await page.locator('#navOnline').click();await page.locator('#onlineAccept:not([disabled])').waitFor();
  assert.equal(await page.evaluate(()=>window.testCalls.filter(c=>c.command==='set_online_league_consent').length),0);
  assert.match(await page.locator('#tabOnline').innerText(),/no se comparten aplicaciones, tareas, títulos ni capturas/);
  for(const scheme of ['light','dark']){
    await page.emulateMedia({colorScheme:scheme});
    for(const width of [340,370,900]){
      await page.setViewportSize({width,height:700});
      await page.screenshot({path:new URL(`consent-${scheme}-${width}.png`,out).pathname.slice(1)});
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      const positions=await page.locator('.bottom-nav .nav-item').evaluateAll(items=>items.map(x=>x.getBoundingClientRect().left));
      assert.ok(positions.every((x,i)=>i===0||x>positions[i-1]));
    }
  }
  await page.setViewportSize({width:370,height:700});
  await page.locator('#onlineAlias').fill('Ana.Foco');
  await page.evaluate(()=>window.onlineFail=true);
  await page.locator('#onlineAccept').click();await page.locator('.online-status').filter({hasText:'Sin conexión'}).waitFor();
  assert.equal(await page.locator('#onlineCreate').count(),0);
  await page.evaluate(()=>window.onlineFail=false);
  await page.evaluate(()=>window.usernameTaken=true);
  await page.locator('#onlineAlias').fill('Ana.Foco');
  await page.locator('#onlineAccept').click();await page.locator('.online-status').filter({hasText:'ya está ocupado'}).waitFor();
  assert.equal(await page.locator('#onlineAlias').inputValue(),'ana.foco');
  assert.equal(await page.locator('#onlineCreate').count(),0);
  await page.evaluate(()=>window.usernameTaken=false);
  await page.locator('#onlineAlias').fill('Ana.Foco');
  await page.locator('#onlineAccept').click();await page.locator('#onlineCreate').waitFor();
  const accepted=await page.evaluate(()=>window.testCalls.filter(c=>c.command==='set_online_league_consent').at(-1));
  assert.deepEqual(accepted.args,{accept:true,alias:'ana.foco'});
  await page.locator('#onlineGroupName').fill('Equipo <b>');await page.locator('#onlineCreate button').click();
  await page.locator('.online-group').waitFor();assert.equal(await page.locator('.online-group h2').innerText(),'Equipo <b>');
  assert.equal(await page.locator('.online-group b,.online-group script').count(),0);
  await page.locator('#onlineInvitationCode').waitFor();assert.equal((await page.locator('#onlineInvitationCode').inputValue()).length,64);
  await page.screenshot({path:new URL('group-simulated-native.png',out).pathname.slice(1)});

  // Accepted once: automatic status and username, with no manual sync action.
  assert.equal(await page.locator('#onlineRefresh').count(),0);
  assert.match(await page.locator('.online-status').innerText(),/Sincronización automática/);
  assert.match(await page.locator('.online-status').innerText(),/25 min/);
  assert.equal(await page.locator('.online-username').innerText(),'@ana.foco');

  // Native background events refresh even while the user stays on Clock.
  await page.locator('#navToday').click();
  await page.evaluate(()=>{
    Object.assign(window.onlineTestState,{today_points:40,eligible_minutes:25,synced_at:'2026-10-05T10:01:00Z'});
    window.onlineTestState.groups[0].members[0].points=40;
    window.testEmit('online-leagues-updated');
  });
  await page.waitForFunction(()=>document.querySelector('.online-score-line strong')?.textContent==='40 / 100');
  assert.equal(await page.locator('#tabToday').evaluate(tab=>tab.classList.contains('active')),true);
  await page.locator('#navOnline').click();
  assert.equal(await page.locator('.online-group li strong').innerText(),'40');
  assert.equal(await page.locator('.online-status').getAttribute('data-state'),'success');

  // Preserve drafts/focus when a background result arrives during editing.
  await page.locator('.online-sharing summary').click();
  await page.locator('#onlineGroupName').fill('Mi nuevo grupo');
  await page.locator('#onlineCode').fill('codigo-pendiente');
  await page.locator('#onlineGroupName').focus();
  await page.evaluate(()=>{window.onlineHold=true;window.testEmit('online-leagues-updated');});
  await page.waitForFunction(()=>typeof window.finishOnlineRefresh==='function');
  await page.locator('#onlineGroupName').fill('Mi grupo editado durante la petición');
  await page.evaluate(()=>{window.onlineTestState.today_points=70;window.onlineTestState.eligible_minutes=50;window.onlineTestState.groups[0].members[0].points=70;window.testEmit('online-leagues-updated');window.onlineHold=false;window.finishOnlineRefresh();});
  await page.waitForFunction(()=>document.querySelector('.online-score-line strong')?.textContent==='70 / 100');
  assert.equal(await page.locator('#onlineGroupName').inputValue(),'Mi grupo editado durante la petición');
  assert.equal(await page.locator('#onlineCode').inputValue(),'codigo-pendiente');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'onlineGroupName');
  assert.equal(await page.locator('.online-sharing').evaluate(detail=>detail.open),true);
  for(const width of [340,370,900]){
    await page.setViewportSize({width,height:700});
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  }

  // Offline automatic attempts preserve the ranking; a later event recovers.
  await page.evaluate(()=>{window.onlineCloudError='Online leagues could not connect. Check your connection and retry';window.testEmit('online-leagues-updated');});
  await page.locator('.online-status').filter({hasText:'Sin conexión'}).waitFor();
  assert.equal(await page.locator('.online-status').getAttribute('data-state'),'error');
  assert.equal(await page.locator('.online-group li strong').innerText(),'70');
  assert.equal(await page.locator('.online-score-line strong').innerText(),'70 / 100');
  await page.evaluate(()=>{window.onlineCloudError='';window.onlineFail=true;window.testEmit('online-leagues-updated');});
  await page.locator('.online-status').filter({hasText:'Sin conexión'}).waitFor();
  await page.evaluate(()=>{window.onlineFail=false;window.testEmit('online-leagues-updated');});
  await page.locator('.online-status[data-state="success"]').waitFor();
  await page.locator('.online-sharing summary').click();
  await page.locator('#navToday').click();await page.locator('#navOnline').click();
  await page.locator('.online-summary').evaluate(el=>el.scrollIntoView());
  await page.setViewportSize({width:370,height:700});
  await page.screenshot({path:new URL('automatic-sync-simulated-native.png',out).pathname.slice(1)});

  // A response from before withdrawal must not restore enabled consent.
  await page.locator('.online-sharing summary').click();
  await page.evaluate(()=>{window.onlineHold=true;window.finishOnlineRefresh=null;window.testEmit('online-leagues-updated');});
  await page.waitForFunction(()=>typeof window.finishOnlineRefresh==='function');
  await page.locator('#onlineDisable').click();
  await page.locator('#onlineAccept').waitFor();
  await page.evaluate(()=>window.finishOnlineRefresh());
  await page.waitForTimeout(50);
  assert.equal(await page.locator('#onlineInvitationCode').count(),0);
  assert.equal(await page.locator('.online-summary').count(),0);
  assert.equal(await page.locator('#onlineRefresh').count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS: Online opt-in, unique username, group creation, invitation, escaping, withdrawal; automatic sync without Online open or manual buttons, username association, native events, offline retries, updated ranking, draft/focus preservation and stale response protection; five-tab layout at 340/370/900 in light/dark. Screenshots use a simulated native service.');
}finally{await browser.close();}
