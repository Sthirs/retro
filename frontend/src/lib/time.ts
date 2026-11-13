const DAY = 24 * 60 * 60 * 1000;
const HOUR = 60 * 60 * 1000;
const MINUTE = 60 * 1000;

/**
 * "in 6 days", "in 3 hours", "tomorrow"… for a future instant, through
 * `Intl.RelativeTimeFormat` in the given locale.
 */
export function relativeFromNow(iso: string, locale: string, now: number = Date.now()): string {
  const diff = new Date(iso).getTime() - now;
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (Math.abs(diff) >= DAY) return format.format(Math.round(diff / DAY), 'day');
  if (Math.abs(diff) >= HOUR) return format.format(Math.round(diff / HOUR), 'hour');
  return format.format(Math.max(1, Math.round(diff / MINUTE)), 'minute');
}
