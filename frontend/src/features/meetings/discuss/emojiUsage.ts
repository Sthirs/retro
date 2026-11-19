/**
 * Which emoji a user reacts with most (spec §9.5): the first row of the
 * reaction picker shows them, and its "Recently used" tab the latest ones.
 * Kept in this browser, per user (`retro.emojiUsage.{userId}`); without
 * storage the picker shows the defaults.
 */

const KEY = 'retro.emojiUsage';
/** Emoji in the picker's first row. */
export const QUICK_COUNT = 8;
/** Emoji kept in "Recently used". */
export const RECENT_COUNT = 24;
/** Fills the first row while the user has reacted with fewer emoji. */
export const DEFAULT_QUICK = ['👍', '❤️', '😂', '🎉', '😮', '😬', '💯', '🙏'];

export interface EmojiUsage {
  /** How many times each emoji was used. */
  counts: Record<string, number>;
  /** Latest first. */
  recent: string[];
}

const empty = (): EmojiUsage => ({ counts: {}, recent: [] });

/** The user's usage; empty when none is stored or it is unreadable. */
export function readUsage(userId: string): EmojiUsage {
  try {
    const raw = localStorage.getItem(`${KEY}.${userId}`);
    if (!raw) return empty();
    const parsed = JSON.parse(raw) as Partial<EmojiUsage>;
    const counts: Record<string, number> = {};
    if (parsed.counts && typeof parsed.counts === 'object') {
      for (const [emoji, n] of Object.entries(parsed.counts)) {
        if (typeof n === 'number' && Number.isFinite(n) && n > 0) counts[emoji] = n;
      }
    }
    const recent = Array.isArray(parsed.recent)
      ? parsed.recent.filter((e): e is string => typeof e === 'string').slice(0, RECENT_COUNT)
      : [];
    return { counts, recent };
  } catch {
    return empty();
  }
}

/** Counts one use of `emoji` by the user. */
export function recordUsage(userId: string, emoji: string): EmojiUsage {
  const usage = readUsage(userId);
  usage.counts[emoji] = (usage.counts[emoji] ?? 0) + 1;
  usage.recent = [emoji, ...usage.recent.filter((e) => e !== emoji)].slice(0, RECENT_COUNT);
  try {
    localStorage.setItem(`${KEY}.${userId}`, JSON.stringify(usage));
  } catch {
    // Storage unavailable: the row stays as it is.
  }
  return usage;
}

/**
 * The first row: the most used emoji (ties: the more recent first), then the
 * defaults, `QUICK_COUNT` in all.
 */
export function mostUsed(usage: EmojiUsage, count = QUICK_COUNT): string[] {
  const recency = (e: string) => {
    const i = usage.recent.indexOf(e);
    return i < 0 ? Number.MAX_SAFE_INTEGER : i;
  };
  const used = Object.keys(usage.counts).sort(
    (a, b) => usage.counts[b]! - usage.counts[a]! || recency(a) - recency(b),
  );
  const row: string[] = [];
  for (const e of [...used, ...DEFAULT_QUICK]) {
    if (row.length >= count) break;
    if (!row.includes(e)) row.push(e);
  }
  return row;
}
