<script setup lang="ts">
/**
 * A group in the Vote phase (Vote artboard, spec §9.4), with the viewer's
 * own votes on it. Collapsed: its name, card count, a two-line preview, the
 * cards' column colours and the vote stepper. Expanded (local to the
 * viewer): every card. A card that was never grouped got a group of its own
 * when Vote started; it shows as the plain card it was.
 */
import { computed, nextTick, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import type { GroupDto, ReflectionDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import RichText from '@/components/RichText.vue';
import { useGroupView } from '@/features/meetings/group/groupView';
import { plainText } from '@/features/meetings/group/plainText';
import { voteLabel } from '@/features/meetings/vote/voteLabel';
import VoteStepper from '@/features/meetings/vote/VoteStepper.vue';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{
  group: GroupDto;
  cards: ReflectionDto[];
  /**
   * What tells this group from another of the same name (its column and
   * place), for its vote controls; empty when the name is unique.
   */
  where?: string;
}>();
const { t } = useI18n();
const store = useMeetingStore();
const view = useGroupView();

const single = computed(() => props.cards.length === 1 && !props.group.name);
const preview = computed(() =>
  props.cards[0] ? plainText(props.cards[0].content, t('group.image')) : '',
);
/** What the region is called. */
const label = computed(() =>
  voteLabel(props.group, props.cards, { untitled: t('group.untitled'), image: t('group.image') }),
);
/** What the vote controls are called: unique in the room. */
const controlName = computed(() =>
  props.where ? t('vote.controlName', { label: label.value, where: props.where }) : label.value,
);
const voted = computed(() => (store.myVotes.get(props.group.id) ?? 0) > 0);
const expanded = computed(() => view.expanded.has(props.group.id));

const toggleButton = ref<HTMLButtonElement | null>(null);

/** Expands or collapses; the toggle keeps the focus as it re-renders. */
async function toggle(): Promise<void> {
  view.toggle(props.group.id);
  await nextTick();
  toggleButton.value?.focus();
}
</script>

<template>
  <article
    v-if="single"
    class="vcard"
    :class="{ 'vcard--voted': voted }"
    :aria-label="label"
    :style="{ '--fg': `var(--col-${cards[0]!.column}-fg)` }"
    data-testid="vote-group"
  >
    <span class="vcard__column">
      <span class="vcard__dot" aria-hidden="true" />
      {{ t(`reflect.columns.${cards[0]!.column}.name`) }}
    </span>
    <RichText class="vcard__text" :html="cards[0]!.content" />
    <div class="vgroup__stepper">
      <VoteStepper :group-id="group.id" :name="controlName" />
    </div>
  </article>
  <section
    v-else
    class="vgroup"
    :class="{ 'vgroup--open': expanded, 'vgroup--voted': voted }"
    :aria-label="label"
    data-testid="vote-group"
  >
    <span v-if="!expanded" class="vgroup__layer vgroup__layer--far" aria-hidden="true" />
    <span v-if="!expanded" class="vgroup__layer vgroup__layer--near" aria-hidden="true" />
    <div class="vgroup__panel">
      <button
        ref="toggleButton"
        type="button"
        class="vgroup__toggle"
        :aria-expanded="expanded"
        data-testid="vote-group-toggle"
        @click="toggle"
      >
        <span class="vgroup__name">{{ label }}</span>
        <span class="vgroup__count">{{ t('group.cards', cards.length) }}</span>
        <span class="vgroup__chevron" aria-hidden="true">
          <AppIcon name="chevronDown" :size="16" />
        </span>
      </button>
      <template v-if="expanded">
        <div
          v-for="card in cards"
          :key="card.id"
          class="vcard vcard--inner"
          :style="{ '--fg': `var(--col-${card.column}-fg)` }"
          data-testid="vote-card"
        >
          <span class="vcard__column">
            <span class="vcard__dot" aria-hidden="true" />
            {{ t(`reflect.columns.${card.column}.name`) }}
          </span>
          <RichText class="vcard__text" :html="card.content" />
        </div>
        <div class="vgroup__stepper">
          <VoteStepper :group-id="group.id" :name="controlName" />
        </div>
      </template>
      <template v-else>
        <p class="vgroup__preview">{{ preview }}</p>
        <div class="vgroup__foot">
          <span class="vgroup__dots" aria-hidden="true">
            <span
              v-for="card in cards"
              :key="card.id"
              class="vgroup__dot"
              :style="{ background: `var(--col-${card.column}-fg)` }"
            />
          </span>
          <VoteStepper :group-id="group.id" :name="controlName" />
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.vgroup {
  position: relative;
  margin-bottom: 10px;
}
.vgroup--open {
  margin-bottom: 0;
}
.vgroup__layer {
  position: absolute;
  height: 20px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
}
.vgroup__layer--far {
  left: 10px;
  right: 10px;
  bottom: -10px;
  opacity: 0.6;
}
.vgroup__layer--near {
  left: 5px;
  right: 5px;
  bottom: -5px;
  opacity: 0.85;
}
.vgroup__panel {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 12px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--shadow-card);
}
.vgroup--voted .vgroup__panel {
  padding: 7px 11px 11px;
  border: 2px solid var(--accent);
}
.vgroup--open .vgroup__panel {
  padding: 8px 10px 10px;
  border: 2px solid var(--accent);
  border-radius: var(--radius-panel);
  background: var(--accent-soft);
  box-shadow: none;
}
.vgroup__toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 44px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  text-align: left;
  cursor: pointer;
}
.vgroup__name {
  flex: 1 1 auto;
  min-width: 0;
  overflow-wrap: anywhere;
  font-size: 15px;
  font-weight: 700;
}
.vgroup__count {
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
.vgroup__chevron {
  display: flex;
  color: var(--accent-text);
}
.vgroup--open .vgroup__chevron {
  transform: rotate(180deg);
}
.vgroup__preview {
  display: -webkit-box;
  margin: 0;
  overflow: hidden;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.4;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.vgroup__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.vgroup__dots {
  display: flex;
  flex: 0 1 auto;
  align-items: center;
  min-width: 0;
  padding: 2px;
  overflow: hidden;
}
.vgroup__dot {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  box-shadow: 0 0 0 2px var(--card);
}
.vgroup__dot + .vgroup__dot {
  margin-left: -4px;
}
.vgroup__stepper {
  display: flex;
  justify-content: flex-end;
}
.vcard {
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
.vcard--voted {
  padding: 9px 11px;
  border: 2px solid var(--accent);
}
.vcard__column {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--fg);
  font-size: 11px;
  font-weight: 700;
}
.vcard__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--fg);
}
.vcard__text {
  color: var(--text);
  font-size: 14px;
  line-height: 1.45;
}
</style>
