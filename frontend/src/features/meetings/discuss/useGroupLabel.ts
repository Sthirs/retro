import { useI18n } from 'vue-i18n';

import { voteLabel } from '@/features/meetings/vote/voteLabel';
import { useMeetingStore } from '@/stores/meeting';

/**
 * What a group is called in Discuss: its name, a never-grouped card's text,
 * or "Untitled group" (as in Vote).
 */
export function useGroupLabel(): (groupId: string) => string {
  const { t } = useI18n();
  const store = useMeetingStore();
  return (groupId) => {
    const group = store.meeting?.groups.find((g) => g.id === groupId);
    const cards = store.groupCards.get(groupId) ?? [];
    if (!group) return t('group.untitled');
    return voteLabel(group, cards, { untitled: t('group.untitled'), image: t('group.image') });
  };
}
