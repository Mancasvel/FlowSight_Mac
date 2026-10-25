// Actual installed MV3 extension, synthetic local bridge and intercepted sites.
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {mkdtemp,mkdir,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {chromium} from 'playwright';

const folder=await mkdtemp(join(process.env.FLOWSIGHT_TEST_TEMP || tmpdir(),'flowsight-extension-'));
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
// Extension-created tabs cannot have their first request intercepted by Playwright.
// Serve it through a local HTTP proxy on a free port; port 80 is reserved on
// Windows CI runners. The browser still navigates to the exact public-shaped URL.
const wav=Buffer.alloc(44+30*8000*2);
wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);
wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);
wav.writeUInt32LE(8000,24);wav.writeUInt32LE(16000,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);
wav.write('data',36);wav.writeUInt32LE(wav.length-44,40);
const musicSite=createServer((req,res)=>{
  if(req.url.endsWith('/tone.wav')){res.setHeader('Content-Type','audio/wav');res.end(wav);return;}
  res.setHeader('Content-Type','text/html');
  res.end('<h1>Synthetic music site</h1><video muted controls loop src="/tone.wav"></video>');
});
await new Promise((resolve,reject)=>{musicSite.once('error',reject);musicSite.listen(0,'127.0.0.1',resolve);});
const extension=resolve('apps/agent/browser-extension');
let context;
try{
  context=await chromium.launchPersistentContext(folder,{channel:'chromium',headless:true,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`,`--proxy-server=http://127.0.0.1:${musicSite.address().port}`,'--proxy-bypass-list=127.0.0.1;localhost']});
  await context.route('https://*.example.com/**',route=>route.fulfill({contentType:'text/html',body:'<h1>Synthetic work site</h1>'}));
  const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker');
  const extensionId=new URL(worker.url()).host;
  const options=await context.newPage(); await options.goto(`chrome-extension://${extensionId}/options.html`);
  await options.locator('#port').fill(String(server.address().port));
  await options.locator('#token').fill('wrong-key');
  await options.locator('button[type=submit]').click();
  await options.locator('#status').filter({hasText:'pairing key was rejected'}).waitFor();
  await options.locator('#token').fill('synthetic-key');
  await options.locator('button[type=submit]').click();
  await options.locator('#status').filter({hasText:'Connected to FlowSight'}).waitFor();
  assert.equal(status.extensionVersion,'1.1.4');
  assert.equal(status.musicAvailable,true);
  await options.screenshot({path:join(output,'browser-pairing-confirmed.png')});
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
  await page.evaluate(()=>history.pushState({},'', '/feed'));
  await page.waitForURL(`chrome-extension://${extensionId}/blocked.html`);
  await page.goto('https://distraction.example.com/work/docs');
  const next=await context.newPage();await next.goto('https://distraction.example.com/feed').catch(()=>{});
  await next.waitForURL(`chrome-extension://${extensionId}/blocked.html`);
  connected=false;await pollNow();
  assert.equal((await worker.evaluate(()=>focusStatus())).applied,true,'A temporary disconnect must not remove the protection.');
  connected=true;await next.locator('#end').click();
  await next.locator('#end').waitFor({state:'hidden'});
  await pollNow();assert.equal(focus,null);
  await next.goto('https://distraction.example.com/feed');assert.match(await next.locator('h1').innerText(),/Synthetic/);
  const musicUrl='http://music.example.com/listen?playlist=focus&track=123';
  const before=await worker.evaluate(async()=>({tabs:await chrome.tabs.query({active:true}),windows:await chrome.windows.getAll({windowTypes:['normal']})}));
  focus={id:'test-music',intention:'Synthetic background music check',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['music.example.com'],exceptions:[],musicUrl};
  await pollNow();
  const musicResult=await worker.evaluate(()=>runCommand('browser.focus_status',{openMusic:true,sessionId:'test-music'}));
  assert.equal(musicResult.musicOpened,true);
  let music;
  for(let attempt=0;attempt<50;attempt++) {
    music=context.pages().find(page=>page.url()===musicUrl);
    if(music)break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.ok(music, `Music page missing; browser pages: ${context.pages().map(page=>page.url()).join(', ')}`);
  await music.waitForURL(musicUrl);await music.locator('h1').filter({hasText:'Synthetic music site'}).waitFor();
  const after=await worker.evaluate(async()=>({music:await chrome.tabs.query({url:'http://music.example.com/listen*'}),tabs:await chrome.tabs.query({active:true}),windows:await chrome.windows.getAll({windowTypes:['normal']})}));
  assert.equal(after.music.length,1);assert.equal(after.music[0].active,false);
  assert.deepEqual(after.tabs.map(tab=>tab.id),before.tabs.map(tab=>tab.id));
  assert.deepEqual(after.windows.map(window=>[window.id,window.focused]),before.windows.map(window=>[window.id,window.focused]));
  await worker.evaluate(()=>runCommand('browser.focus_status',{openMusic:true,sessionId:'test-music'}));
  assert.equal((await worker.evaluate(()=>chrome.tabs.query({url:'http://music.example.com/listen*'}))).length,1);
  await next.goto('http://music.example.com/listen?playlist=other').catch(()=>{});
  await next.waitForURL(`chrome-extension://${extensionId}/blocked.html`);
  focus=null;await pollNow();
  focus={id:'test-music-resume',intention:'Synthetic resume check',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['music.example.com'],exceptions:[],musicUrl};
  await pollNow();
  assert.equal((await worker.evaluate(()=>chrome.tabs.query({url:'http://music.example.com/listen*'}))).length,1);
  // Public-shaped YouTube hostname served entirely by the local proxy. This
  // exercises the real content script and HTML media playback, not a player mock.
  focus=null;await pollNow();
  const youtubeUrl='http://www.youtube.com/watch?v=synthetic&list=focus';
  focus={id:'youtube-start',intention:'Synthetic YouTube playback',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['youtube.com'],exceptions:[],musicUrl:youtubeUrl};
  await pollNow();
  const youtubeOpen=await worker.evaluate(()=>runCommand('browser.focus_status',{openMusic:true,sessionId:'youtube-start'}));
  assert.equal(youtubeOpen.musicOpened,true);
  let youtube;
  for(let attempt=0;attempt<50;attempt++){
    youtube=context.pages().find(page=>page.url()===youtubeUrl);
    if(youtube)break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  assert.ok(youtube);
  await youtube.waitForFunction(()=>document.querySelector('video')?.paused===false);
  await youtube.waitForFunction(()=>document.querySelector('video')?.currentTime>0.2);
  focus=null;await pollNow();
  const paused=await worker.evaluate(()=>runCommand('browser.focus_status',{musicPlayback:'pause',musicSessionId:'youtube-start',sessionId:'youtube-start'}));
  assert.equal(paused.musicPlayback,'pause');
  await youtube.waitForFunction(()=>document.querySelector('video')?.paused===true);
  const position=await youtube.locator('video').evaluate(video=>video.currentTime);
  await youtube.locator('video').evaluate(video=>video.play());
  assert.equal(await youtube.locator('video').evaluate(video=>video.paused),true,'Late autoplay stays paused');
  focus={...focus,id:'youtube-resume',intention:'Synthetic YouTube playback',expiresAt:new Date(Date.now()+300000).toISOString(),patterns:['youtube.com'],exceptions:[],musicUrl:youtubeUrl};
  await pollNow();
  const resumed=await worker.evaluate(()=>runCommand('browser.focus_status',{musicPlayback:'play',musicSessionId:'youtube-start',sessionId:'youtube-resume'}));
  assert.equal(resumed.musicPlayback,'play');
  await youtube.waitForFunction(()=>document.querySelector('video')?.paused===false);
  assert.ok(await youtube.locator('video').evaluate(video=>video.currentTime)>=position);
  assert.equal((await worker.evaluate(()=>chrome.tabs.query({url:'http://www.youtube.com/watch*'}))).length,1);
  await youtube.locator('video').evaluate(video=>video.pause());
  await pollNow();
  assert.equal(await youtube.locator('video').evaluate(video=>video.paused),true,'Status checks preserve manual pause');
  const final=await worker.evaluate(async()=>({tabs:await chrome.tabs.query({active:true}),windows:await chrome.windows.getAll({windowTypes:['normal']})}));
  assert.deepEqual(final.tabs.map(tab=>tab.id),before.tabs.map(tab=>tab.id));
  assert.deepEqual(final.windows.map(window=>[window.id,window.focused]),before.windows.map(window=>[window.id,window.focused]));
  focus=null;await pollNow();
  await worker.evaluate(()=>runCommand('browser.focus_status',{musicPlayback:'release',musicSessionId:'youtube-resume',sessionId:'youtube-resume'}));
  await youtube.locator('video').evaluate(video=>video.play());
  assert.equal(await youtube.locator('video').evaluate(video=>video.paused),false);
  focus={id:'test-expiry',intention:'Synthetic expiry check',expiresAt:new Date(Date.now()+60000).toISOString(),patterns:['distraction.example.com'],exceptions:[]};
  await pollNow();
  await worker.evaluate(async()=>{const {focus}=await chrome.storage.local.get('focus');focus.expiresAt=new Date(Date.now()-1).toISOString();await chrome.storage.local.set({focus});await expireBlocks();});
  assert.equal((await worker.evaluate(()=>focusStatus())).applied,false);
  console.log('Actual MV3 extension: YouTube media play/pause/resume at the same position, late autoplay, manual controls, background tab/window focus, exact music URL exception, no duplicates, blocking, navigation, disconnect, emergency end and expiry passed.');
}finally{
  if(context)await context.close();await new Promise(resolve=>server.close(resolve));await new Promise(resolve=>musicSite.close(resolve));await rm(folder,{recursive:true,force:true});
}
