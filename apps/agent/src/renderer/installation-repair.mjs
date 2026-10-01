import { t as tr, message as formatMessage, html, markup, setText, setAttributeText, getLocale, getLanguage, initializeLocalization } from './i18n.mjs';
/** Friendly, automatic repair for damaged packaged Windows installations. */
export function createInstallationRepairController({ invoke, listen, document, isMicrosoftStoreBuild }) {
  let repairNeeded = false;
  let repairing = false;
  let automaticRepairAttempted = false;

  async function check() {
    try {
      const result = await invoke('check_installation_health');
      repairNeeded = result?.healthy !== true;
      if (!repairNeeded) automaticRepairAttempted = false;
    } catch {
      console.warn('[Installation] Check could not finish.');
      repairNeeded = true;
    }
    if (repairNeeded) show();
    return !repairNeeded;
  }

  function show() {
    if (!repairNeeded || document.getElementById('repairOverlay')) return;

    const overlay = document.createElement('div');
    overlay.id = 'repairOverlay';
    overlay.className = 'modal-overlay update-dialog-overlay';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');
    overlay.setAttribute('aria-labelledby', 'repairDialogTitle');
    overlay.setAttribute('aria-describedby', 'repairDialogDescription');
    overlay.innerHTML = html`
      <div class="modal-content update-dialog">
        <div class="update-dialog__header">
          <span class="update-dialog__icon" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M12 3a9 9 0 1 0 9 9"/><path d="M12 3v5h5"/><path d="M9 12l2 2 4-4"/>
            </svg>
          </span>
          <div>
            <h2 class="update-dialog__title" id="repairDialogTitle">FlowSight needs a repair</h2>
            <p class="update-dialog__version">Your personal data stays in place</p>
          </div>
        </div>
        <div class="modal-body update-dialog__body" tabindex="0" role="region" aria-label="Repair details">
          <p class="update-dialog__description" id="repairDialogDescription">${isMicrosoftStoreBuild
            ? tr('Some app files need replacing. Reinstall FlowSight from your Microsoft Store Library to restore them.')
            : tr('Some app files need replacing. FlowSight will download a verified copy and reinstall the app automatically.')}</p>
        </div>
        <div class="update-dialog__footer">
          <p class="update-dialog__hint">Your activity history, settings and license are kept outside the app installation.</p>
          <p class="update-dialog__error" id="repairError" role="alert" hidden></p>
          <div class="update-dialog__progress" id="repairProgress" tabindex="-1" hidden>
            <div class="update-dialog__progress-track" id="repairProgressTrack" role="progressbar" aria-label="Repair download" aria-valuemin="0" aria-valuemax="100">
              <div class="update-dialog__progress-fill" id="repairProgressBar"></div>
            </div>
            <p class="update-dialog__progress-label" id="repairProgressLabel" role="status">Preparing repair…</p>
          </div>
          <div class="update-dialog__actions" id="repairActions">
            <button type="button" class="button button-secondary" id="repairCloseBtn">Use app for now</button>
            ${isMicrosoftStoreBuild ? '' : markup('<button type="button" class="button button-primary" id="repairRetryBtn">Retry repair</button>')}
          </div>
        </div>
      </div>`;
    document.body.appendChild(overlay);
    setText(overlay.querySelector('#repairDialogDescription'), () => tr(isMicrosoftStoreBuild ? 'Some app files need replacing. Reinstall FlowSight from your Microsoft Store Library to restore them.' : 'Some app files need replacing. FlowSight will download a verified copy and reinstall the app automatically.'));

    const closeButton = overlay.querySelector('#repairCloseBtn');
    const retryButton = overlay.querySelector('#repairRetryBtn');
    const actions = overlay.querySelector('#repairActions');
    const progress = overlay.querySelector('#repairProgress');
    const track = overlay.querySelector('#repairProgressTrack');
    const bar = overlay.querySelector('#repairProgressBar');
    const label = overlay.querySelector('#repairProgressLabel');
    const errorMessage = overlay.querySelector('#repairError');

    closeButton.addEventListener('click', () => {
      if (repairing) return;
      overlay.remove();
    });
    overlay.addEventListener('keydown', event => {
      if (event.key === 'Escape' && !repairing) {
        event.preventDefault();
        overlay.remove();
      }
    });

    async function repair() {
      if (repairing) return;
      repairing = true;
      actions.hidden = true;
      errorMessage.hidden = true;
      progress.hidden = false;
      setText(label, () => (tr('Downloading a verified copy…')));
      progress.focus();
      let unlistenProgress = null;
      let unlistenInstalling = null;
      try {
        unlistenProgress = await listen('installation-repair-progress', ({ payload }) => {
          const percent = Math.max(0, Math.min(100, Number(payload) || 0));
          bar.style.transform = `scaleX(${percent / 100})`;
          track.setAttribute('aria-valuenow', String(percent));
          setText(label, () => (formatMessage`Downloading a verified copy… ${percent}%`));
        }).catch(() => null);
        unlistenInstalling = await listen('installation-repair-installing', () => {
          bar.style.transform = 'scaleX(1)';
          track.setAttribute('aria-valuenow', '100');
          setText(label, () => (tr('Reinstalling FlowSight… it will restart shortly.')));
        }).catch(() => null);
        await invoke('repair_installation');
      } catch {
        console.warn('[Installation] Repair could not finish.');
        setText(errorMessage, () => (tr('The repair could not finish. Check your connection and try again.')));
        errorMessage.hidden = false;
        progress.hidden = true;
        actions.hidden = false;
        closeButton.focus();
      } finally {
        unlistenProgress?.();
        unlistenInstalling?.();
        repairing = false;
      }
    }

    retryButton?.addEventListener('click', () => { repair().catch(() => {}); });
    if (isMicrosoftStoreBuild) {
      setText(closeButton, () => (tr('Close')));
      closeButton.focus();
    } else if (!automaticRepairAttempted) {
      automaticRepairAttempted = true;
      repair().catch(() => {});
    } else {
      retryButton?.focus();
    }
  }

  return { check, show, needsRepair: () => repairNeeded };
}
