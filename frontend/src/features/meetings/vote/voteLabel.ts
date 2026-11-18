import type { GroupDto, ReflectionDto } from '@/api/client';
import { excerpt, plainText } from '@/features/meetings/group/plainText';

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
  return excerpt(cards[0] ? plainText(cards[0].content, words.image) : '');
}
