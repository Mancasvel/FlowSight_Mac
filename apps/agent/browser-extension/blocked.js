const es = (navigator.language || '').toLowerCase().startsWith('es');
const copy = es ? {
  heading:'Un espacio para concentrarte',description:'Esta página está bloqueada durante tu sesión de concentración.',
  end:'Finalizar concentración total',help:'Puedes cambiar las páginas bloqueadas y las excepciones en los ajustes de FlowSight.',
  ended:'La protección ha terminado. Puedes volver a abrir la página.',until:'Hasta',error:'No se pudo finalizar. Inténtalo otra vez.',
} : {heading:'Room for your focus',description:'This page is blocked during your focus session.',end:'End total focus',
  help:'You can change blocked sites and exceptions in FlowSight Settings.',ended:'Protection has ended. You can reopen the page.',until:'Until',error:'Could not end protection. Try again.'};
document.documentElement.lang = es ? 'es' : 'en';
for (const id of ['heading','description','end','help']) document.getElementById(id).textContent = copy[id];
const button = document.getElementById('end');
async function render() {
  const { focus } = await chrome.storage.local.get('focus');
  const active = focus && Date.parse(focus.expiresAt) > Date.now();
  document.getElementById('task').textContent = active ? focus.intention : '';
  document.getElementById('until').textContent = active ? `${copy.until} ${new Date(focus.expiresAt).toLocaleTimeString(es ? 'es-ES' : 'en-GB',{hour:'2-digit',minute:'2-digit'})}` : copy.ended;
  button.hidden = !active;
}
button.onclick = async () => {
  button.disabled = true;
  try { const result = await chrome.runtime.sendMessage({ type:'end-focus' }); if (result?.error) throw new Error(result.error); await render(); }
  catch { document.getElementById('status').textContent = copy.error; }
  finally { button.disabled = false; }
};
render();setInterval(render,15000);
