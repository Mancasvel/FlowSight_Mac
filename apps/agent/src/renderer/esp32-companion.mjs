/** Transport only. The host supplies its own timer state, translations and actions. */
export async function mountEsp32Companion({ invoke, listen, snapshot, execute, events = document }) {
  let publishing = false, executing = false;
  const publish = async () => {
    if (publishing) return;
    publishing = true;
    try { const state = snapshot(); if (state) await invoke('update_esp32_companion', { snapshot: state }); }
    catch (error) { console.warn('[USB companion] Snapshot unavailable', error); }
    finally { publishing = false; }
  };
  const stopRefresh = await listen('esp32-companion-refresh', publish);
  const stopControl = await listen('esp32-companion-control', async ({ payload }) => {
    let ok = false;
    if (!executing) {
      executing = true;
      try { ok = Boolean(await execute(payload.action)); }
      catch (error) { console.warn('[USB companion] Action failed', error); }
      finally { executing = false; }
    }
    await publish();
    try { await invoke('complete_esp32_companion_action', { connection: payload.connection, id: payload.id, ok }); }
    catch (error) { console.warn('[USB companion] Acknowledgement unavailable', error); }
  });
  // Let existing language listeners update the goal and button labels first.
  const languageChanged = () => queueMicrotask(publish);
  events.addEventListener('flowsight:languagechange', languageChanged);
  await publish();
  return () => { stopRefresh(); stopControl(); events.removeEventListener('flowsight:languagechange', languageChanged); };
}
