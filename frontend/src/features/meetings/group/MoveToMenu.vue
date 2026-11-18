<script setup lang="ts">
/**
 * A card's "Move to…" menu (spec §9.3): the keyboard way to group, as drag
 * and drop is pointer only. It lists the other groups, the ungrouped cards a
 * new group can start with and, for a grouped card, "Remove from group".
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ReflectionDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import { usePopover } from '@/composables/usePopover';
import { useGroupView } from '@/features/meetings/group/groupView';
import { excerpt, plainText } from '@/features/meetings/group/plainText';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ card: ReflectionDto; disabled?: boolean }>();
const { t } = useI18n();
const store = useMeetingStore();
const view = useGroupView();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const menu = ref<HTMLElement | null>(null);
const { open, close } = usePopover(root, trigger);

function cardExcerpt(html: string): string {
  return excerpt(plainText(html, t('group.image')));
}

const groups = computed(() =>
  (store.meeting?.groups ?? [])
    .filter((g) => g.id !== props.card.groupId)
    .map((g) => {
      const cards = store.groupCards.get(g.id) ?? [];
      return {
        id: g.id,
        name: g.name || t('group.untitled'),
        // Untitled groups are told apart by their first card.
        excerpt: !g.name && cards[0] ? cardExcerpt(cards[0].content) : '',
        count: cards.length,
      };
    }),
);
const loose = computed(() =>
  (store.meeting?.reflections ?? [])
    .filter((r) => !r.groupId && r.id !== props.card.id)
    .map((r) => ({ id: r.id, column: r.column, text: cardExcerpt(r.content) })),
);

const PANEL_WIDTH = 280;
const EDGE = 8;
const GAP = 4;
const placement = ref<Record<string, string>>({});

/** Fixed below the button, or above it when there is more room there. */
function place(): void {
  const button = trigger.value;
  if (!button) return;
  const rect = button.getBoundingClientRect();
  const left = Math.max(
    EDGE,
    Math.min(rect.right - PANEL_WIDTH, window.innerWidth - PANEL_WIDTH - EDGE),
  );
  const below = window.innerHeight - rect.bottom - GAP - EDGE;
  const above = rect.top - GAP - EDGE;
  placement.value =
    below >= above
      ? { left: `${left}px`, top: `${rect.bottom + GAP}px`, maxHeight: `${below}px` }
      : {
          left: `${left}px`,
          bottom: `${window.innerHeight - rect.top + GAP}px`,
          maxHeight: `${above}px`,
        };
}

function onViewportChange(event: Event): void {
  if (event.target instanceof Node && root.value?.contains(event.target)) return;
  close();
}

watch(open, (isOpen) => {
  if (isOpen) {
    window.addEventListener('resize', onViewportChange);
    document.addEventListener('scroll', onViewportChange, true);
  } else {
    window.removeEventListener('resize', onViewportChange);
    document.removeEventListener('scroll', onViewportChange, true);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener('resize', onViewportChange);
  document.removeEventListener('scroll', onViewportChange, true);
});

function items(): HTMLElement[] {
  return [...(menu.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
}

function focusItem(index: number): void {
  const list = items();
  const item = list[(index + list.length) % list.length];
  if (!item) return;
  // The panel is fixed: scroll only within it, never the page behind.
  item.focus({ preventScroll: true });
  const panel = menu.value;
  if (!panel) return;
  if (item.offsetTop < panel.scrollTop) panel.scrollTop = item.offsetTop;
  else if (item.offsetTop + item.offsetHeight > panel.scrollTop + panel.clientHeight) {
    panel.scrollTop = item.offsetTop + item.offsetHeight - panel.clientHeight;
  }
}

async function toggle(): Promise<void> {
  open.value = !open.value;
  if (!open.value) return;
  await nextTick();
  place();
  focusItem(0);
}

function onKeyDown(event: KeyboardEvent): void {
  const index = items().indexOf(document.activeElement as HTMLElement);
  switch (event.key) {
    case 'ArrowDown':
      focusItem(index + 1);
      break;
    case 'ArrowUp':
      focusItem(index < 0 ? -1 : index - 1);
      break;
    case 'Home':
      focusItem(0);
      break;
    case 'End':
      focusItem(-1);
      break;
    default:
      return;
  }
  event.preventDefault();
}

function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (next && root.value && !root.value.contains(next)) close();
}

/**
 * Runs a move, then shows the card where it is (moved, or back after a
 * refusal) and focuses it again: this menu may have unmounted meanwhile.
 */
async function run(move: () => Promise<boolean>): Promise<void> {
  close();
  const id = props.card.id;
  await move();
  const card = store.meeting?.reflections.find((r) => r.id === id);
  view.reveal(id, card?.groupId ?? null);
}

function intoGroup(groupId: string): void {
  void run(() => store.moveToGroup(props.card.id, groupId));
}

function withCard(targetId: string): void {
  void run(() => store.dropOnCard(targetId, props.card.id));
}

function ungroup(): void {
  void run(() => store.moveToGroup(props.card.id, null));
}
</script>

<template>
  <div ref="root" class="move" @focusout="onFocusOut">
    <button
      ref="trigger"
      type="button"
      class="move__trigger"
      data-move-trigger
      aria-haspopup="menu"
      :aria-expanded="open"
      :disabled="disabled"
      @click="toggle"
    >
      <AppIcon name="move" :size="14" />
      {{ t('group.moveTo') }}
    </button>
    <div
      v-if="open"
      ref="menu"
      class="move__panel"
      role="menu"
      :aria-label="t('group.moveTo')"
      :style="{ position: 'fixed', ...placement }"
      @keydown="onKeyDown"
    >
      <button v-if="card.groupId" type="button" role="menuitem" class="item" @click="ungroup">
        <AppIcon name="logout" :size="16" />
        {{ t('group.removeFromGroup') }}
      </button>
      <template v-if="groups.length > 0">
        <div class="move__heading" role="presentation">{{ t('group.intoGroup') }}</div>
        <button
          v-for="group in groups"
          :key="group.id"
          type="button"
          role="menuitem"
          class="item"
          @click="intoGroup(group.id)"
        >
          <AppIcon name="grid" :size="16" />
          <span class="item__text">
            <span class="item__label">{{ group.name }}</span>
            <span v-if="group.excerpt" class="item__excerpt">{{ group.excerpt }}</span>
          </span>
          <span class="item__count">{{ t('group.cards', group.count) }}</span>
        </button>
      </template>
      <template v-if="loose.length > 0">
        <div class="move__heading" role="presentation">{{ t('group.newGroupWith') }}</div>
        <button
          v-for="other in loose"
          :key="other.id"
          type="button"
          role="menuitem"
          class="item"
          @click="withCard(other.id)"
        >
          <span
            class="item__dot"
            aria-hidden="true"
            :style="{ background: `var(--col-${other.column}-fg)` }"
          />
          <span class="item__label">{{ other.text }}</span>
        </button>
      </template>
      <p v-if="!card.groupId && groups.length === 0 && loose.length === 0" class="move__empty">
        {{ t('group.nowhere') }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.move {
  position: relative;
  display: flex;
  justify-content: flex-end;
}
.move__trigger {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 30px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}
.move__trigger::before {
  /* 44 px touch target around the 30 px control. */
  content: '';
  position: absolute;
  inset: -7px 0;
}
.move__trigger:hover,
.move__trigger[aria-expanded='true'] {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.move__trigger:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}
.move__panel {
  z-index: 10;
  display: flex;
  flex-direction: column;
  gap: 2px;
  box-sizing: border-box;
  width: 280px;
  padding: 6px;
  overflow-y: auto;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-menu);
  cursor: default;
}
.move__heading {
  padding: 8px 12px 4px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
}
.move__empty {
  margin: 0;
  padding: 10px 12px;
  color: var(--muted);
  font-size: 13px;
}
.item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 44px;
  padding: 0 12px;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
}
.item:hover,
.item:focus-visible {
  background: var(--well);
}
.item__label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item__text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
  padding: 4px 0;
}
.item__text .item__label {
  flex: none;
}
.item__excerpt {
  overflow: hidden;
  color: var(--muted);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item__count {
  flex: none;
  color: var(--muted);
  font-size: 12px;
  font-weight: 600;
}
.item__dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
</style>
