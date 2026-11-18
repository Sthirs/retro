import { afterEach, describe, expect, it } from 'vitest';

import {
  categorize,
  CATEGORY_IDS,
  loadEmojis,
  searchEmojis,
} from '@/features/meetings/discuss/emojiData';
import {
  DEFAULT_QUICK,
  mostUsed,
  QUICK_COUNT,
  readUsage,
  RECENT_COUNT,
  recordUsage,
} from '@/features/meetings/discuss/emojiUsage';

afterEach(() => localStorage.clear());

describe('emoji usage', () => {
  it('starts from the defaults', () => {
    expect(mostUsed(readUsage('u1'))).toEqual(DEFAULT_QUICK);
    expect(DEFAULT_QUICK).toHaveLength(QUICK_COUNT);
  });

  it('puts the most used emoji first, the more recent one on a tie', () => {
    recordUsage('u1', '🚀');
    recordUsage('u1', '🚀');
    recordUsage('u1', '🦄');
    recordUsage('u1', '👍');
    const row = mostUsed(readUsage('u1'));
    expect(row.slice(0, 3)).toEqual(['🚀', '👍', '🦄']);
    // Padded with the defaults, without repeating any.
    expect(row).toHaveLength(QUICK_COUNT);
    expect(new Set(row).size).toBe(QUICK_COUNT);
    expect(row.slice(3)).toEqual(['❤️', '😂', '🎉', '😮', '😬']);
  });

  it('keeps the recent emoji, latest first, without repeats', () => {
    for (const e of ['😀', '🎉', '😀']) recordUsage('u1', e);
    expect(readUsage('u1').recent).toEqual(['😀', '🎉']);
    for (let i = 0; i < RECENT_COUNT + 5; i++) recordUsage('u1', String.fromCodePoint(0x1f600 + i));
    expect(readUsage('u1').recent).toHaveLength(RECENT_COUNT);
  });

  it('is kept per user', () => {
    recordUsage('u1', '🚀');
    expect(mostUsed(readUsage('u2'))).toEqual(DEFAULT_QUICK);
    expect(localStorage.getItem('retro.emojiUsage.u1')).toContain('🚀');
  });

  it('survives broken storage', () => {
    localStorage.setItem('retro.emojiUsage.u1', '{not json');
    expect(readUsage('u1')).toEqual({ counts: {}, recent: [] });
    localStorage.setItem(
      'retro.emojiUsage.u1',
      JSON.stringify({ counts: { '🚀': 'x', '🎉': -1, '👍': 2 }, recent: [1, '👍'] }),
    );
    expect(readUsage('u1')).toEqual({ counts: { '👍': 2 }, recent: ['👍'] });
  });
});

describe('emoji catalogue', () => {
  it('folds the Unicode groups into the six tabs', async () => {
    const categories = await loadEmojis();
    expect(categories.map((c) => c.id)).toEqual([...CATEGORY_IDS]);
    for (const c of categories) expect(c.emojis.length).toBeGreaterThan(50);
    const smileys = categories[0]!.emojis;
    expect(smileys[0]).toEqual({ emoji: '😀', name: 'Grinning face', slug: 'grinning_face' });
  });

  it('leaves out emoji newer than the browsers render', () => {
    const groups = [
      {
        slug: 'smileys_emotion',
        emojis: [
          { emoji: '😀', name: 'grinning face', slug: 'grinning_face', emoji_version: '1.0' },
          { emoji: '🫨', name: 'shaking face', slug: 'shaking_face', emoji_version: '15.0' },
          { emoji: '🙂‍↔️', name: 'head shaking', slug: 'head_shaking', emoji_version: '15.1' },
        ],
      },
      { slug: 'component', emojis: [] },
    ];
    const [smileys] = categorize(groups);
    expect(smileys!.emojis.map((e) => e.emoji)).toEqual(['😀', '🫨']);
  });

  it('finds emoji by name or shortcode', async () => {
    const categories = await loadEmojis();
    expect(searchEmojis(categories, 'party popper')[0]?.emoji).toBe('🎉');
    expect(searchEmojis(categories, ':party_popper:')[0]?.emoji).toBe('🎉');
    expect(searchEmojis(categories, 'THUMBS')[0]?.emoji).toBe('👍');
    expect(searchEmojis(categories, '   ')).toEqual([]);
    expect(searchEmojis(categories, 'zzzqqq')).toEqual([]);
  });
});
