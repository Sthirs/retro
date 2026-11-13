// Keep in sync with src/lib/theme.ts (THEME_STORAGE_KEY).
try {
  var theme = localStorage.getItem('retro.theme');
  if (theme === 'light' || theme === 'dark') {
    document.documentElement.setAttribute('data-theme', theme);
  }
} catch {
  // Storage unavailable: follow the system preference.
}
