<script setup lang="ts">
/**
 * A reflection card in the Group phase (Group artboard, spec §9.3): its
 * column label, its text and Move to…. It can be dragged onto another card
 * or a group. A card someone else is dragging stays in its place, lifted,
 * with their badge, and cannot be picked up (soft lock).
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ParticipantDto, ReflectionDto } from '@/api/client';
import RichText from '@/components/RichText.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { firstName } from '@/features/meetings/firstName';
import MoveToMenu from '@/features/meetings/group/MoveToMenu.vue';
import { useCardDrag } from '@/features/meetings/group/useCardDrag';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ card: ReflectionDto }>();
const { t } = useI18n();
const store = useMeetingStore();
const drag = useCardDrag();

const root = ref<HTMLElement | null>(null);
const dragger = computed<ParticipantDto | undefined>(() => store.draggers.get(props.card.id));
const canMove = computed(() => store.isActive && !dragger.value);
const lifting = computed(() => drag.dragging.value === props.card.id);
const isOver = computed(
  () => drag.over.value?.kind === 'card' && drag.over.value.id === props.card.id,
);

function onPointerDown(event: PointerEvent): void {
  if (!canMove.value || !root.value) return;
  drag.press(event, props.card.id, root.value);
}
</script>

<template>
  <article
    ref="root"
    class="gcard"
    :class="{
      'gcard--movable': canMove,
      'gcard--held': !!dragger,
      'gcard--lifting': lifting,
      'gcard--over': isOver,
    }"
    :style="{ '--fg': `var(--col-${card.column}-fg)` }"
    :data-drop="`card:${card.id}`"
    :data-card-id="card.id"
    :aria-busy="dragger ? 'true' : undefined"
    @pointerdown="onPointerDown"
  >
    <span v-if="dragger" class="gcard__badge" data-testid="drag-badge">
      <span class="gcard__avatar" aria-hidden="true">
        <UserAvatar
          :user-id="dragger.id"
          :name="dragger.displayName"
          :picture-version="dragger.pictureVersion"
          :size="22"
          ring="var(--accent)"
        />
      </span>
      {{ t('group.moving', { name: firstName(dragger.displayName) }) }}
    </span>
    <div class="gcard__head">
      <span class="gcard__column">
        <span class="gcard__dot" aria-hidden="true" />
        {{ t(`reflect.columns.${card.column}.name`) }}
      </span>
      <MoveToMenu v-if="store.isActive" class="gcard__move" :card="card" :disabled="!!dragger" />
    </div>
    <RichText class="gcard__text" :html="card.content" />
  </article>
</template>

<style scoped>
.gcard {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--shadow-card);
  touch-action: manipulation;
}
.gcard--movable {
  cursor: grab;
  user-select: none;
}
.gcard--lifting {
  opacity: 0.4;
}
.gcard--over {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.gcard--held {
  margin-top: 10px;
  border: 2px solid var(--accent);
  box-shadow: var(--shadow-menu);
  transform: rotate(-2deg);
}
.gcard__badge {
  position: absolute;
  top: -14px;
  right: 10px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 10px 2px 2px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 700;
}
.gcard__avatar {
  display: inline-flex;
}
.gcard__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 14px;
}
.gcard__column {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--fg);
  font-size: 11px;
  font-weight: 700;
}
.gcard__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--fg);
}
.gcard__text {
  color: var(--text);
  font-size: 14px;
  line-height: 1.45;
}
.gcard__text :deep(img) {
  max-height: 160px;
}
.gcard__move {
  /* The 30 px button overhangs the label row instead of making it taller. */
  margin: -8px -6px;
  opacity: 0;
}
.gcard:hover .gcard__move,
.gcard__move:focus-within {
  opacity: 1;
}
@media (hover: none) {
  .gcard__move {
    opacity: 1;
  }
}
</style>
