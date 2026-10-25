// Runs only on YouTube. Only the tab opened by Total focus receives a command.
(() => {
  let playback = null;
  let revision = -1;
  let observer = null;
  const bound = new WeakSet();
  const played = new WeakMap();

  async function applyPlayback() {
    const media = Array.from(document.querySelectorAll('video, audio'));
    for (const element of media) {
      if (!bound.has(element)) {
        bound.add(element);
        element.addEventListener('loadedmetadata', () => applyPlayback().catch(() => {}));
        element.addEventListener('play', () => {
          if (playback === 'pause') element.pause();
        });
      }
      if (playback === 'pause') element.pause();
      else if (playback === 'play' && element.paused && played.get(element) !== revision) {
        const requestedRevision = revision;
        played.set(element, requestedRevision);
        try { await element.play(); }
        catch (_) {
          played.delete(element);
          if (requestedRevision === revision && playback === 'play') {
            throw new Error('YouTube could not start playback. Allow autoplay or press Play once in the music tab.');
          }
        }
        if (playback === 'pause') element.pause();
      }
    }
    return { pending: media.length === 0 };
  }

  async function receive(command) {
    if (!['pause', 'play', 'release'].includes(command?.playback) || !Number.isSafeInteger(command.revision)
        || command.revision < revision) return {};
    revision = command.revision;
    playback = command.playback === 'release' ? null : command.playback;
    if (!playback) {
      observer?.disconnect();
      observer = null;
      return {};
    }
    if (!observer) {
      observer = new MutationObserver(() => applyPlayback().catch(() => {}));
      observer.observe(document, { childList: true, subtree: true });
    }
    return applyPlayback();
  }

  chrome.runtime.onMessage.addListener((message, sender, reply) => {
    if (sender.id !== chrome.runtime.id || message?.type !== 'focus-music-playback') return;
    receive(message).then(reply, error => reply({ error: error.message }));
    return true;
  });
  chrome.runtime.sendMessage({ type: 'get-focus-music-playback' })
    .then(receive).catch(() => {});
})();
