import { spanish } from './locales/es.mjs';

const STORAGE_KEY='flowsight_language_preference';
export const normalizeLanguage=value=>/^es(?:[-_]|$)/i.test(String(value||''))?'es':'en';
export function resolveLanguage(preference,languages=['en']) {
 return preference==='es'||preference==='en'?preference:normalizeLanguage(languages[0]);
}
let preference='system';
try {const saved=globalThis.localStorage?.getItem(STORAGE_KEY);if(['system','es','en'].includes(saved))preference=saved;} catch { /* Session-only choice still works. */ }
let language=resolveLanguage(preference,globalThis.navigator?.languages||[globalThis.navigator?.language||'en']);
let systemLanguage=normalizeLanguage(globalThis.navigator?.language||'en');
export const getLanguage=()=>language;
export const getLanguagePreference=()=>preference;
export const getLocale=()=>language==='es'?'es-ES':'en-GB';
const bindings=new Map();
let nativeInvoke=null,revision=0;
const exact=key=>language==='es'?(spanish[key]??key):key;
const canonicalCategories=new Set(['Analysis','Coding','Debugging','Code Review','Testing','Design','DevOps','Database','Research','Documentation','Planning','Communication','Meeting','Admin','Browsing','Idle','General']);
export const categoryLabel=value=>canonicalCategories.has(value)?t(value):value;
export function t(key,values={}) {return exact(key).replace(/\{(\w+)\}/g,(match,name)=>name in values?String(values[name]):match);}
// For app/backend status only, never for task titles or other user-authored data.
export function localizeStatus(value){const source=String(value??'');if(language==='en')return source;
 if(spanish[source])return spanish[source];
 const missing=source.match(/^The local AI missed '(.+)'. It must include each requested topic; try again\.$/);if(missing)return `La IA local omitió '${missing[1]}'. Debe incluir cada tema solicitado; inténtalo de nuevo.`;
 for(const [key,translated]of Object.entries(spanish))if(key.endsWith(':')&&source.startsWith(key+' '))return translated+source.slice(key.length);
 return source;
}
export function message(strings,...values){return t(strings.reduce((s,part,i)=>s+part+(i<values.length?`{p${i}}`:''),''),Object.fromEntries(values.map((v,i)=>[`p${i}`,v])));}
export function setText(node,value) {
 if(!node)return;
 const render=typeof value==='function'?value:()=>value;
 const last=String(render()??'');bindings.set(node,{...bindings.get(node),attribute:null,render,last});node.textContent=last;
}
export function setAttributeText(node,attribute,value) {
 if(!node)return;
 const render=typeof value==='function'?value:()=>value;
 let record=bindings.get(node);if(!record?.attributes){record={...record,attributes:new Map()};bindings.set(node,record);}
 const last=String(render()??'');record.attributes.set(attribute,{render,last});node.setAttribute(attribute,last);
}
function translateMarkupFragment(fragment){
 const translate=(whole,source,prefix='>',suffix='<')=>{
  const normalized=source.replace(/\s+/g,' ').trim();if(!spanish[normalized])return whole;
  const translated=t(normalized),leading=source.match(/^\s*/)[0],trailing=source.match(/\s*$/)[0];return`${prefix}<!--fs-i18n:${encodeURIComponent(normalized)}-->${leading}${translated}${trailing}<!--/fs-i18n-->${suffix}`;
 };
 return fragment.replace(/>([^<>]*)</g,(whole,source)=>translate(whole,source))
 .replace(/^([^<>]+)</,(whole,source)=>translate(whole,source,'','<'))
 .replace(/>([^<>]+)$/,(whole,source)=>translate(whole,source,'>',''))
 .replace(/(placeholder|title|aria-label|alt|data-coach-prompt)="([^"<>]*)"/g,(whole,attr,key)=>spanish[key]?`${attr}="${t(key)}" data-i18n-${attr}="${encodeURIComponent(key)}"`:whole);
}
// Translate only source-owned markup before values are interpolated. Values can never become translation keys.
export function html(strings,...values){return strings.reduce((s,part,i)=>s+translateMarkupFragment(part)+(i<values.length?values[i]:''),'');}
export const markup=source=>translateMarkupFragment(source);
function bindMarkup(root){
 if(!root?.isConnected)return;
 const walker=document.createTreeWalker(root,NodeFilter.SHOW_COMMENT);
 while(walker.nextNode()){
  const comment=walker.currentNode;if(!comment.nodeValue.startsWith('fs-i18n:'))continue;
  const node=comment.nextSibling;if(node?.nodeType!==Node.TEXT_NODE)continue;
  const key=decodeURIComponent(comment.nodeValue.slice(8)),source=node.nodeValue;
  const render=()=>source.replace(source.trim(),t(key));bindings.set(node,{attribute:'nodeValue',render,last:source});
 }
 for(const node of [root,...(root.querySelectorAll?.('*')||[])])for(const attribute of ['placeholder','title','aria-label','alt','data-coach-prompt']){
  const key=node.getAttribute?.(`data-i18n-${attribute}`);if(key)setAttributeText(node,attribute,()=>t(decodeURIComponent(key)));
 }
}
export function localizeStatic(root=document.body) {
 if(!root)return;
 const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);
 while(walker.nextNode()){
  const node=walker.currentNode;if(node.parentElement?.closest('script,style,textarea,[data-user-content]'))continue;
  const source=node.nodeValue,normalized=source.replace(/\s+/g,' ').trim();
  if(spanish[normalized]){const leading=source.match(/^\s*/)[0],trailing=source.match(/\s*$/)[0],render=()=>leading+t(normalized)+trailing;bindings.set(node,{attribute:'nodeValue',render,last:render()});node.nodeValue=render();}
 }
 for(const node of [root,...root.querySelectorAll('[placeholder],[title],[aria-label],[alt],[data-coach-prompt]')])for(const attribute of ['placeholder','title','aria-label','alt','data-coach-prompt']){
  const source=node.getAttribute?.(attribute);if(spanish[source])setAttributeText(node,attribute,()=>t(source));
 }
}
function refreshBindings(){for(const [node,record]of bindings){
 if(!node.isConnected){bindings.delete(node);continue;}
 if(record.render){const current=record.attribute==='nodeValue'?node.nodeValue:node.textContent;
  if(current===record.last){record.last=String(record.render()??'');if(record.attribute==='nodeValue')node.nodeValue=record.last;else node.textContent=record.last;}
 }
 for(const [attribute,binding]of record.attributes||[]){if(node.getAttribute(attribute)===binding.last){binding.last=String(binding.render()??'');node.setAttribute(attribute,binding.last);}}
}}
export function setLanguagePreference(next,{persist=true}={}) {
 if(!['system','es','en'].includes(next))throw new Error('Unsupported language preference.');
 preference=next;language=resolveLanguage(next,[systemLanguage]);
 let saved=true;try{if(persist)globalThis.localStorage?.setItem(STORAGE_KEY,next);}catch{saved=false;}
 if(globalThis.document){document.documentElement.lang=language;refreshBindings();document.dispatchEvent(new CustomEvent('flowsight:languagechange',{detail:{language,preference}}));}
 return saved;
}
export function initializeLocalization(){
 document.documentElement.lang=language;localizeStatic();
 const observer=new MutationObserver(records=>{for(const record of records)if([...record.addedNodes].some(node=>node.nodeType!==Node.TEXT_NODE))bindMarkup(record.target);});
 observer.observe(document.body,{childList:true,subtree:true});
 const select=document.getElementById('languageSelect');if(select){select.value=preference;select.addEventListener('change',()=>{
  const saved=setLanguagePreference(select.value);const current=++revision;
  const persist=nativeInvoke?nativeInvoke('set_language_preference',{preference}).then(()=>true,()=>false):Promise.resolve(saved);
  persist.then(nativeSaved=>{if(current===revision)setText(document.getElementById('languageStatus'),()=>t(saved&&nativeSaved?'Language saved':'This language applies for this session. Could not save it on this device.'));});
 });}
 window.addEventListener('languagechange',()=>{if(!nativeInvoke){systemLanguage=normalizeLanguage(navigator.language);if(preference==='system')setLanguagePreference('system',{persist:false});}});
}
export async function connectNativeLocalization(invoke){
 nativeInvoke=invoke;const current=revision;
 try{const result=await invoke('get_language_preference');if(current!==revision||!result)return;
  // Migrate an existing renderer choice once; otherwise the native persisted preference is authoritative.
  systemLanguage=normalizeLanguage(result.systemLanguage||result.language);
  if(preference!=='system'&&result.preference==='system'&&result.persisted===false){await invoke('set_language_preference',{preference});setLanguagePreference(preference);return;}
  if(['system','es','en'].includes(result.preference)){setLanguagePreference(result.preference);const select=document.getElementById('languageSelect');if(select)select.value=preference;}
 }catch{/* Browser previews or unavailable storage retain this session's language. */}
}
