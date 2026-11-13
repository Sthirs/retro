/** Theme preference. `system` follows `prefers-color-scheme`. */
export type ThemePreference = 'light' | 'dark' | 'system';

/** Keep in sync with public/theme-init.js. */
export const THEME_STORAGE_KEY = 'retro.theme';

export function readThemePreference(
  storage: Pick<Storage, 'getItem'> = localStorage,
): ThemePreference {
  try {
    const value = storage.getItem(THEME_STORAGE_KEY);
    return value === 'light' || value === 'dark' ? value : 'system';
  } catch {
    return 'system';
  }
}

export function resolveTheme(preference: ThemePreference, prefersDark: boolean): 'light' | 'dark' {
  if (preference === 'system') return prefersDark ? 'dark' : 'light';
  return preference;
}

/** Saves the preference and applies it to `<html data-theme>`. */
export function applyThemePreference(
  preference: ThemePreference,
  root: HTMLElement = document.documentElement,
  storage: Pick<Storage, 'setItem' | 'removeItem'> = localStorage,
): void {
  try {
    if (preference === 'system') storage.removeItem(THEME_STORAGE_KEY);
    else storage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // Storage unavailable: the choice lasts for this page only.
  }
  if (preference === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', preference);
}
