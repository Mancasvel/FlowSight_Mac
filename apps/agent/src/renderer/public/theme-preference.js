(() => {
  const storageKey = 'flowsight_theme_preference';
  const root = document.documentElement;
  const darkSheets = [
    document.getElementById('themeDarkBase'),
    document.getElementById('themeDarkMobile'),
  ].filter(Boolean);
  // Vite puts its bundled light stylesheet at the end of <head> in production.
  // Keep the dark overrides after it so changing `media` changes the visible UI.
  for (const sheet of darkSheets) document.head.appendChild(sheet);
  const systemDark = window.matchMedia('(prefers-color-scheme: dark)');

  function savedPreference() {
    try {
      const value = window.localStorage.getItem(storageKey);
      return value === 'light' || value === 'dark' ? value : null;
    } catch {
      return null;
    }
  }

  function applyTheme(theme) {
    const dark = theme === 'dark';
    for (const sheet of darkSheets) sheet.media = dark ? 'all' : 'not all';
    root.dataset.theme = theme;
    root.style.colorScheme = theme;

    const button = document.getElementById('themeToggleBtn');
    if (button) {
      button.setAttribute('aria-pressed', String(dark));
      const spanish = root.lang === 'es';
      button.title = spanish ? (dark ? 'Cambiar al modo claro' : 'Cambiar al modo oscuro') : (dark ? 'Switch to light mode' : 'Switch to dark mode');
      button.setAttribute('aria-label', spanish ? (dark ? 'Modo claro' : 'Modo oscuro') : (dark ? 'Light mode' : 'Dark mode'));
    }
  }

  let manualChoice = savedPreference();
  document.addEventListener('flowsight:languagechange', () => applyTheme(root.dataset.theme));
  applyTheme(manualChoice || (systemDark.matches ? 'dark' : 'light'));

  function connectButton() {
    const button = document.getElementById('themeToggleBtn');
    if (!button) return;
    button.addEventListener('click', () => {
      const next = root.dataset.theme === 'dark' ? 'light' : 'dark';
      manualChoice = next;
      try {
        window.localStorage.setItem(storageKey, next);
      } catch {
        // The switch still works for this session when storage is unavailable.
      }
      applyTheme(next);
    });
    applyTheme(root.dataset.theme);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', connectButton, { once: true });
  } else {
    connectButton();
  }

  const followSystem = (event) => {
    if (!manualChoice) applyTheme(event.matches ? 'dark' : 'light');
  };
  if (systemDark.addEventListener) systemDark.addEventListener('change', followSystem);
  else systemDark.addListener(followSystem);
})();
