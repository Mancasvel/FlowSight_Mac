const portInput = document.getElementById('port');
const tokenInput = document.getElementById('token');
const status = document.getElementById('status');
const form = document.getElementById('pairForm');
const storage = globalThis.chrome?.storage?.local;
portInput.value = '38547';

if (!storage) {
  status.textContent = 'This page is not running as an installed browser extension. When FlowSight offers an official store link, install FlowSight Browser Controls there and open Extension options from the installed extension.';
  form.querySelector('button[type="submit"]').disabled = true;
} else {
  storage.get(['port', 'token']).then(({ port, token }) => {
    if (port) portInput.value = String(port);
    if (token) tokenInput.value = token;
  }).catch((error) => {
    status.textContent = `Could not read saved pairing details: ${error.message}`;
  });

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    const port = Number(portInput.value);
    const token = tokenInput.value.trim();
    if (!Number.isInteger(port) || port < 1 || port > 65535 || !token) {
      status.textContent = 'Enter the port and pairing key shown in FlowSight.';
      return;
    }
    try {
      await storage.set({ port, token });
      status.textContent = 'Checking the connection…';
      const connection = await globalThis.chrome.runtime.sendMessage({ type: 'poll-now' });
      status.textContent = connection?.connected && connection.totalFocusAvailable ? 'Connected to FlowSight. Browser actions and total focus are ready.'
        : connection?.connected ? `Connected to FlowSight. ${connection.error || 'Website protection is unavailable. Check site access in the extension settings.'}`
        : connection?.error || 'No response from the extension. Reopen its options and try again.';
    } catch (error) {
      status.textContent = `Could not connect: ${error.message}`;
    }
  });
}
