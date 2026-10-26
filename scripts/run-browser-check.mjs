// Bound each integration process and identify the last script if a browser hangs.
import {spawn} from 'node:child_process';
import {resolve} from 'node:path';

const script=process.argv[2];
if(!script)throw new Error('Usage: node scripts/run-browser-check.mjs <check.mjs>');
const timeout=Number(process.env.FLOWSIGHT_BROWSER_CHECK_TIMEOUT_MS||240000);
if(!Number.isFinite(timeout)||timeout<1||timeout>900000)throw new Error('Invalid browser-check timeout');
const label=`${script} (${process.env.FLOWSIGHT_E2E_BROWSER||'chromium'})`;
const started=Date.now();
console.log(`START ${label}; process deadline ${timeout/1000}s`);
const child=spawn(process.execPath,[resolve(script)],{stdio:'inherit',windowsHide:true,detached:process.platform!=='win32'});
let expired=false;
let forceTimer;
const stop=signal=>{
  try {
    if(process.platform==='win32')child.kill(signal);
    else process.kill(-child.pid,signal); // Only the process group started above.
  }catch(error){if(error.code!=='ESRCH')console.error(error.message);}
};
const timer=setTimeout(()=>{
  expired=true;
  console.error(`TIMEOUT ${label}; stopping its owned process group`);
  stop('SIGTERM');
  forceTimer=setTimeout(()=>stop('SIGKILL'),2000);
},timeout);
try {
  const result=await new Promise((resolveExit,reject)=>{
    child.once('error',reject);
    child.once('exit',(code,signal)=>resolveExit({code,signal}));
  });
  clearTimeout(timer);
  clearTimeout(forceTimer);
  if(expired)stop('SIGKILL');
  console.log(`END ${label}; elapsed ${((Date.now()-started)/1000).toFixed(1)}s; ${expired?'deadline exceeded':`exit ${result.code}, signal ${result.signal||'none'}`}`);
  process.exitCode=expired?1:(result.code??1);
}catch(error){
  clearTimeout(timer);
  clearTimeout(forceTimer);
  stop('SIGKILL');
  throw error;
}
