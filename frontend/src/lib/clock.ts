/** Longest timer: 99:59 (spec §8.2). */
export const MAX_TIMER_SECONDS = 99 * 60 + 59;

/** `mm:ss` for a countdown, rounding up so it shows 00:00 only at the end. */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

/**
 * A typed duration in seconds: `mm:ss`, `m:ss` or plain minutes (`5`).
 * `null` when it is not one, or outside 1 s–99:59.
 */
export function parseDuration(text: string): number | null {
  const value = text.trim();
  let seconds: number;
  const clock = /^(\d{1,2}):(\d{2})$/.exec(value);
  if (clock) {
    const s = Number(clock[2]);
    if (s > 59) return null;
    seconds = Number(clock[1]) * 60 + s;
  } else if (/^\d{1,2}$/.test(value)) {
    seconds = Number(value) * 60;
  } else {
    return null;
  }
  return seconds >= 1 && seconds <= MAX_TIMER_SECONDS ? seconds : null;
}
