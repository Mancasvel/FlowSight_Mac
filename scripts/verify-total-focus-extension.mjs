// Actual installed MV3 extension, synthetic local bridge and intercepted sites.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {mkdtemp,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {chromium} from 'playwright';

const folder=await mkdtemp(join(tmpdir(),'flowsight-extension-'));
const output=resolve('.impeccable/review');await mkdir(output,{recursive:true});
let focus=null,status=null,connected=true;
const server=createServer(async(req,res)=>{
  res.setHeader('Access-Control-Allow-Origin','*');res.setHeader('Access-Control-Allow-Headers','X-FlowSight-Token, Content-Type');
  if(req.method==='OPTIONS'){res.writeHead(204);res.end();return;}
  if(req.headers['x-flowsight-token']!=='synthetic-key'){res.writeHead(401);res.end();return;}
  if(!connected){res.writeHead(503);res.end();return;}
  res.setHeader('Content-Type','application/json');
  if(req.url==='/next'){res.end(JSON.stringify({command:null,focus}));return;}
  if(req.url==='/focus_status'){
    let body='';for await(const chunk of req)body+=chunk;status=JSON.parse(body);
    if(status.cancelledSessionId===focus?.id)focus=null;
    res.end('{"received":true}');return;
  }
  res.end('{"received":true}');
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const extension=resolve('apps/agent/browser-extension');
let context;
try{
  context=await chromium.launchPersistentContext(folder,{channel:'chromium',headless:true,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
  await context.route('https://*.example.com/**',route=>route.fulfill({contentType:'text/html',body:'<h1>Synthetic work site</h1>'}));
  const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker');
  const extensionId=new URL(worker.url()).host;
  const pollNow=()=>worker.evaluate(async()=>{while(polling)await new Promise(resolve=>setTimeout(resolve,20));await poll();});
  await worker.evaluate(async(port)=>{await chrome.storage.local.set({port,token:'synthetic-key'});},server.address().port);
  const page=await context.newPage();await page.goto('https://distraction.example.com/feed');
  focus={id:'test-1',intention:'Synthetic ADDA exercise',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['distraction.example.com'],exceptions:['distraction.example.com/work']};
  await pollNow();
  await page.waitForURL(`chrome-extension://${extensionId}/blocked.html`);
  assert.equal(status.applied,true);assert.equal(status.sessionId,'test-1');
  await page.locator('#task').filter({hasText:'Synthetic ADDA exercise'}).waitFor();
  await page.screenshot({path:join(output,'total-focus-extension-blocked.png')});
  await page.goto('https://distraction.example.com/work/docs');await page.locator('h1').filter({hasText:'Synthetic work site'}).waitFor();
  const next=await context.newPage();await next.goto('https://distraction.example.com/feed').catch(()=>{});
  await next.waitForURL(`chrome-extension://${extensionId}/blocked.html`);
  connected=false;await pollNow();
  assert.equal((await worker.evaluate(()=>focusStatus())).applied,true,'A temporary disconnect must not remove the protection.');
  connected=true;await next.locator('#end').click();
  await next.locator('#end').waitFor({state:'hidden'});
  await pollNow();assert.equal(focus,null);
  await next.goto('https://distraction.example.com/feed');assert.match(await next.locator('h1').innerText(),/Synthetic/);
  focus={id:'test-expiry',intention:'Synthetic expiry check',expiresAt:new Date(Date.now()+60000).toISOString(),patterns:['distraction.example.com'],exceptions:[]};
  await pollNow();
  await worker.evaluate(async()=>{const {focus}=await chrome.storage.local.get('focus');focus.expiresAt=new Date(Date.now()-1).toISOString();await chrome.storage.local.set({focus});await expireBlocks();});
  assert.equal((await worker.evaluate(()=>focusStatus())).applied,false);
  console.log('Actual MV3 extension: open-tab blocking, navigation redirect, path exception, disconnect, emergency end, and expiry passed.');
}finally{
  if(context)await context.close();await new Promise(resolve=>server.close(resolve));await rm(folder,{recursive:true,force:true});
}
