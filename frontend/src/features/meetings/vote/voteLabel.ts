import type { GroupDto, ReflectionDto } from '@/api/client';
import { plainText } from '@/features/meetings/group/plainText';

/** Longest card text used as the name of a single card, in characters. */
const SHORT_NAME = 40;

/**
 * What a group is called in Vote (its region and its stepper): its name, a
 * never-grouped card's text, or `untitled`. Two groups can share it.
 */
export function voteLabel(
  group: GroupDto,
  cards: ReflectionDto[],
  words: { untitled: string; image: string },
): string {
  if (group.name) return group.name;
  if (cards.length !== 1) return words.untitled;
  const chars = [...(cards[0] ? plainText(cards[0].content, words.image) : '')];
  return chars.length > SHORT_NAME ? `${chars.slice(0, SHORT_NAME).join('')}…` : chars.join('');
}
