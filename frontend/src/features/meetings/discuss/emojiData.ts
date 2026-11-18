/**
 * The emoji catalogue of the reaction picker (ADR-0014): every Unicode emoji
 * from unicode-emoji-json, in the picker's six tabs (ReactionPicker artboard).
 * Loaded on first use, so the room does not carry it until someone reacts.
 */

/** One emoji of the picker. */
export interface EmojiEntry {
  emoji: string;
  /** Its Unicode name, capitalized ("Party popper"). */
  name: string;
  /** Its slug, shown as a `:shortcode:` ("party_popper"). */
  slug: string;
}

/** The picker's tabs after "Recently used", in order. */
export const CATEGORY_IDS = ['smileys', 'people', 'nature', 'food', 'objects', 'symbols'] as const;
export type CategoryId = (typeof CATEGORY_IDS)[number];

export interface EmojiCategory {
  id: CategoryId;
  emojis: EmojiEntry[];
}

/** Unicode groups, folded into the design's six tabs. */
const GROUPS: Record<string, CategoryId> = {
  smileys_emotion: 'smileys',
  people_body: 'people',
  animals_nature: 'nature',
  food_drink: 'food',
  travel_places: 'objects',
  activities: 'objects',
  objects: 'objects',
  symbols: 'symbols',
  flags: 'symbols',
};

/**
 * Newest emoji version offered: later ones render as boxes in browsers that
 * do not have them yet (the server accepts them all).
 */
export const MAX_EMOJI_VERSION = 15.0;

interface RawEmoji {
  emoji: string;
  name: string;
  slug: string;
  emoji_version: string;
}
interface RawGroup {
  slug: string;
  emojis: RawEmoji[];
}

const capitalize = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);

/** Builds the tabs from unicode-emoji-json's `data-by-group.json`. */
export function categorize(groups: RawGroup[], maxVersion = MAX_EMOJI_VERSION): EmojiCategory[] {
  const by = new Map<CategoryId, EmojiEntry[]>(CATEGORY_IDS.map((id) => [id, []]));
  for (const group of groups) {
    const id = GROUPS[group.slug];
    if (!id) continue;
    for (const e of group.emojis) {
      if (Number.parseFloat(e.emoji_version) > maxVersion) continue;
      by.get(id)!.push({ emoji: e.emoji, name: capitalize(e.name), slug: e.slug });
    }
  }
  return CATEGORY_IDS.map((id) => ({ id, emojis: by.get(id)! }));
}

let loading: Promise<EmojiCategory[]> | null = null;

/** The catalogue, loaded once. A failed load is tried again next time. */
export function loadEmojis(): Promise<EmojiCategory[]> {
  loading ??= import('unicode-emoji-json/data-by-group.json')
    .then((data) => categorize((data as { default: RawGroup[] }).default))
    .catch((e: unknown) => {
      loading = null;
      throw e;
    });
  return loading;
}

/**
 * Emoji whose name or slug matches every word of `query` (case-insensitive),
 * names starting with the query first.
 */
export function searchEmojis(categories: EmojiCategory[], query: string, limit = 80): EmojiEntry[] {
  const words = query
    .trim()
    .toLowerCase()
    .replace(/:/g, '')
    .split(/[\s_]+/)
    .filter(Boolean);
  if (words.length === 0) return [];
  const first = words.join(' ');
  const found: EmojiEntry[] = [];
  for (const category of categories) {
    for (const e of category.emojis) {
      const hay = `${e.name.toLowerCase()} ${e.slug}`;
      if (words.every((w) => hay.includes(w))) found.push(e);
    }
  }
  const starts = (e: EmojiEntry) =>
    e.name.toLowerCase().startsWith(first) || e.slug.startsWith(first.replace(/ /g, '_'));
  return found.sort((a, b) => Number(starts(b)) - Number(starts(a))).slice(0, limit);
}
