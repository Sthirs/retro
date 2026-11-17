/**
 * What one viewer's Group phase keeps for itself (spec §9.3): which groups
 * are expanded. Expanding is local, never shared.
 */
import { inject, provide, reactive, type InjectionKey } from 'vue';

export interface GroupView {
  expanded: Set<string>;
  toggle: (groupId: string) => void;
  /** Expands a group and focuses a card's Move to… button once it renders. */
  reveal: (cardId: string, groupId: string | null) => void;
}

const KEY: InjectionKey<GroupView> = Symbol('groupView');

export function provideGroupView(focusCard: (cardId: string) => void): GroupView {
  const expanded = reactive(new Set<string>());
  const view: GroupView = {
    expanded,
    toggle(groupId) {
      if (expanded.has(groupId)) expanded.delete(groupId);
      else expanded.add(groupId);
    },
    reveal(cardId, groupId) {
      if (groupId) expanded.add(groupId);
      focusCard(cardId);
    },
  };
  provide(KEY, view);
  return view;
}

export function useGroupView(): GroupView {
  const view = inject(KEY);
  if (!view) throw new Error('useGroupView needs provideGroupView in an ancestor');
  return view;
}
