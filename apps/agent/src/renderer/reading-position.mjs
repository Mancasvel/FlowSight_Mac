/** Keep the same visible resource when a live list inserts newer items above it. */
export function captureReadingPosition(root) {
  if (!root || !root.closest('.tab-panel')?.classList.contains('active')) return null;
  const scroller = root.closest('.tab-content');
  if (!scroller) return null;
  const top = scroller.getBoundingClientRect().top;
  const bottom = top + scroller.clientHeight;
  const anchors = [...root.querySelectorAll('[data-reading-key]')];
  const anchor = anchors.find(node => {
    const rect = node.getBoundingClientRect();
    return rect.bottom > top + 16 && rect.top < bottom - 80;
  });
  const active = root.contains(document.activeElement) ? document.activeElement : null;
  return {
    scroller, scrollTop: scroller.scrollTop,
    key: anchor?.dataset.readingKey,
    offset: anchor ? anchor.getBoundingClientRect().top - top : 0,
    focusId: active?.id,
  };
}

export function restoreReadingPosition(root, position) {
  if (!position) return;
  const { scroller } = position;
  scroller.scrollTop = position.scrollTop;
  const anchor = [...root.querySelectorAll('[data-reading-key]')].find(node => node.dataset.readingKey === position.key);
  if (anchor) {
    const delta = anchor.getBoundingClientRect().top - scroller.getBoundingClientRect().top - position.offset;
    scroller.scrollTop += delta;
  }
  if (position.focusId) root.ownerDocument.getElementById(position.focusId)?.focus({ preventScroll: true });
}

export function historyRenderKey(history, week, language, goalHours) {
  const { tracking, ...observations } = history || {};
  return JSON.stringify([observations, week, language, goalHours]);
}
