/** Accepts only local paths (`/teams/…`): never `//host`, URLs or backslashes. */
export function safeRedirect(target: unknown, fallback = '/'): string {
  if (typeof target !== 'string') return fallback;
  if (!target.startsWith('/') || target.startsWith('//') || target.includes('\\')) return fallback;
  return target;
}
