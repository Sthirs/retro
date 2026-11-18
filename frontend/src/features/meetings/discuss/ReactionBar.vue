<script setup lang="ts">
/**
 * A card's reactions in Discuss (Discuss and ReactionPicker artboards): one
 * chip per emoji with its count, pressed when it is the viewer's; pressing a
 * chip toggles the viewer's reaction (spec §9.5: one of each emoji per
 * person). "Add reaction" opens the emoji picker. Read-only while the viewer
 * previews another group, or once the meeting ended.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ReactionDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import EmojiPicker from '@/features/meetings/discuss/EmojiPicker.vue';
import { recordUsage } from '@/features/meetings/discuss/emojiUsage';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

const props = defineProps<{
  reflectionId: string;
  reactions: ReactionDto[];
  readonly: boolean;
}>();
const { t } = useI18n();
const store = useMeetingStore();
const session = useSessionStore();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const mine = computed(() => props.reactions.filter((r) => r.mine).map((r) => r.emoji));

function toggle(emoji: string): void {
  if (props.readonly) return;
  const adding = !mine.value.includes(emoji);
  if (adding && session.user) recordUsage(session.user.id, emoji);
  void store.toggleReaction(props.reflectionId, emoji);
}

async function close(focusTrigger = true): Promise<void> {
  open.value = false;
  if (focusTrigger) {
    await nextTick();
    trigger.value?.focus();
  }
}

function pick(emoji: string): void {
  toggle(emoji);
  void close();
}

/** A press outside the picker closes it. */
function onPointerDown(event: PointerEvent): void {
  if (root.value && !root.value.contains(event.target as Node)) void close(false);
}
watch(open, (isOpen) => {
  if (isOpen) document.addEventListener('pointerdown', onPointerDown, true);
  else document.removeEventListener('pointerdown', onPointerDown, true);
});
// Previewing another group, or the meeting ending, makes the card read-only.
watch(
  () => props.readonly,
  (readonly) => {
    if (readonly) open.value = false;
  },
);
onBeforeUnmount(() => document.removeEventListener('pointerdown', onPointerDown, true));
</script>

<template>
  <div class="reactions" :aria-label="t('discuss.reactionsOn')" role="group">
    <button
      v-for="r in reactions"
      :key="r.emoji"
      type="button"
      class="chip"
      :class="{ 'chip--mine': r.mine }"
      :aria-pressed="r.mine"
      :aria-label="t('discuss.reaction', { emoji: r.emoji, count: r.count })"
      :disabled="readonly"
      data-testid="reaction-chip"
      :data-emoji="r.emoji"
      @click="toggle(r.emoji)"
    >
      <span aria-hidden="true">{{ r.emoji }}</span>
      <span aria-hidden="true">{{ r.count }}</span>
    </button>
    <span v-if="!readonly" ref="root" class="reactions__add">
      <button
        ref="trigger"
        type="button"
        class="add"
        :aria-label="t('discuss.addReaction')"
        aria-haspopup="dialog"
        :aria-expanded="open"
        data-testid="add-reaction"
        @click="open ? close() : (open = true)"
      >
        <AppIcon name="smile" :size="16" />
      </button>
      <EmojiPicker v-if="open" :mine="mine" @pick="pick" @close="close()" />
    </span>
  </div>
</template>

<style scoped>
.reactions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.chip:disabled {
  cursor: default;
}
.chip:hover:not(:disabled) {
  background: var(--well);
}
.chip--mine,
.chip--mine:hover:not(:disabled) {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.reactions__add {
  position: relative;
  display: inline-flex;
}
.add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: 1px dashed var(--line);
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.add:hover,
.add[aria-expanded='true'] {
  border-color: var(--accent);
  color: var(--accent-text);
}
</style>
