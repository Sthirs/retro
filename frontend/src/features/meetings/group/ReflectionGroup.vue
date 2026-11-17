<script setup lang="ts">
/**
 * A group in the Group phase (Group artboard, spec §9.3). Collapsed: its
 * name, card count, a stacked preview with the first card's text and the
 * cards' column colours. Expanded: every card, each of which can be dragged
 * out. Any participant renames it; the last name saved wins.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import type { GroupDto, ReflectionDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import GroupCard from '@/features/meetings/group/GroupCard.vue';
import { useGroupView } from '@/features/meetings/group/groupView';
import { plainText } from '@/features/meetings/group/plainText';
import { useCardDrag } from '@/features/meetings/group/useCardDrag';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ group: GroupDto; cards: ReflectionDto[] }>();
const { t } = useI18n();
const store = useMeetingStore();
const view = useGroupView();
const drag = useCardDrag();

/** Longest group name, in characters (spec §9.3). */
const NAME_MAX = 80;

const expanded = computed(() => view.expanded.has(props.group.id));
const label = computed(() => props.group.name || t('group.untitled'));
const preview = computed(() =>
  props.cards[0] ? plainText(props.cards[0].content, t('group.image')) : '',
);
const isOver = computed(
  () => drag.over.value?.kind === 'group' && drag.over.value.id === props.group.id,
);

/** The name being typed; follows the group's name while not being edited. */
const draft = ref(props.group.name);
const editing = ref(false);
watch(
  () => props.group.name,
  (name) => {
    if (!editing.value) draft.value = name;
  },
);

function save(): void {
  editing.value = false;
  const name = draft.value.trim();
  draft.value = name;
  if (name !== props.group.name) void store.renameGroup(props.group.id, name);
}

function onKeyDown(event: KeyboardEvent): void {
  const input = event.target as HTMLInputElement;
  if (event.key === 'Enter') {
    event.preventDefault();
    input.blur();
  } else if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    draft.value = props.group.name;
    editing.value = false;
    input.blur();
  }
}

function onBlur(): void {
  if (editing.value) save();
}
</script>

<template>
  <section
    class="group"
    :class="{ 'group--open': expanded, 'group--over': isOver }"
    :aria-label="label"
    :data-drop="`group:${group.id}`"
    data-testid="group"
  >
    <span v-if="!expanded" class="group__layer group__layer--far" aria-hidden="true" />
    <span v-if="!expanded" class="group__layer group__layer--near" aria-hidden="true" />
    <div class="group__panel">
      <div class="group__head">
        <input
          v-model="draft"
          class="group__name"
          type="text"
          :aria-label="t('group.nameLabel')"
          :placeholder="t('group.untitled')"
          :maxlength="NAME_MAX"
          :readonly="!store.isActive"
          @focus="editing = true"
          @keydown="onKeyDown"
          @blur="onBlur"
        />
        <span class="group__count">{{ t('group.cards', cards.length) }}</span>
        <button
          v-if="expanded"
          type="button"
          class="group__collapse"
          aria-expanded="true"
          :aria-label="t('group.collapse')"
          @click="view.toggle(group.id)"
        >
          <AppIcon name="chevronDown" :size="16" />
        </button>
      </div>
      <template v-if="expanded">
        <GroupCard v-for="card in cards" :key="card.id" :card="card" />
      </template>
      <template v-else>
        <p class="group__preview">{{ preview }}</p>
        <div class="group__foot">
          <span class="group__dots" aria-hidden="true">
            <span
              v-for="card in cards"
              :key="card.id"
              class="group__dot"
              :style="{ background: `var(--col-${card.column}-fg)` }"
            />
          </span>
          <button
            type="button"
            class="group__expand"
            aria-expanded="false"
            @click="view.toggle(group.id)"
          >
            {{ t('group.expand') }}
            <AppIcon name="chevronDown" :size="14" />
          </button>
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.group {
  position: relative;
  margin-bottom: 10px;
}
.group--open {
  margin-bottom: 0;
}
.group__layer {
  position: absolute;
  height: 20px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
}
.group__layer--far {
  left: 10px;
  right: 10px;
  bottom: -10px;
  opacity: 0.6;
}
.group__layer--near {
  left: 5px;
  right: 5px;
  bottom: -5px;
  opacity: 0.85;
}
.group__panel {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--shadow-card);
}
.group--open .group__panel {
  padding: 10px;
  border: 2px solid var(--accent);
  border-radius: var(--radius-panel);
  background: var(--accent-soft);
  box-shadow: none;
}
.group--over .group__panel {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.group__head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.group__name {
  flex: 1 1 auto;
  min-width: 0;
  height: 36px;
  box-sizing: border-box;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--card);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 15px;
  font-weight: 700;
}
.group__name::placeholder {
  color: var(--muted);
}
.group__count {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--well);
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.group--open .group__count {
  background: var(--accent);
  color: var(--on-accent);
}
.group__collapse {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  padding: 0;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--accent-text);
  cursor: pointer;
  transform: rotate(180deg);
}
.group__collapse::before {
  content: '';
  position: absolute;
  inset: -4px;
}
.group__preview {
  margin: 0;
  overflow: hidden;
  color: var(--muted);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.group__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.group__dots {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.group__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.group__expand {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 0;
  border: none;
  background: transparent;
  color: var(--accent-text);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.group__expand::before {
  /* 44 px touch target around the text button. */
  content: '';
  position: absolute;
  inset: -6px -8px;
}
</style>
