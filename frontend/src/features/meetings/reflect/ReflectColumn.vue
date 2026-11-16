<script setup lang="ts">
/**
 * One Reflect column: its heading and how many reflections are mine, the
 * composer ("Share your thoughts…"), my sent reflections, and the team strip
 * that shows only how many others sent and are writing (spec §9.2).
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { ReflectionColumn } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import ReflectionCard from '@/features/meetings/reflect/ReflectionCard.vue';
import ReflectionComposer from '@/features/meetings/reflect/ReflectionComposer.vue';
import { MAX_MINI_CARDS } from '@/features/meetings/reflect/columns';
import { useMeetingStore } from '@/stores/meeting';
import { useReflectDraftsStore } from '@/stores/reflectDrafts';

const props = defineProps<{ column: ReflectionColumn }>();
const { t } = useI18n();
const store = useMeetingStore();
const drafts = useReflectDraftsStore();

const name = computed(() => t(`reflect.columns.${props.column}.name`));
const mine = computed(() => store.myReflections.filter((r) => r.column === props.column));
const composing = computed(
  () => drafts.open?.mode === 'new' && drafts.open.column === props.column,
);
const editingId = computed(() => (drafts.open?.mode === 'edit' ? drafts.open.reflectionId : null));

/** Sent by others: every sent reflection here minus mine. */
const fromTeam = computed(() =>
  Math.max(0, (store.meeting?.reflectionCounts[props.column] ?? 0) - mine.value.length),
);
/** Being written by others: the meeting's total minus this tab's own editor. */
const writing = computed(() => {
  const total = store.meeting?.writing[props.column] ?? { new: 0, edit: 0 };
  const open = drafts.open?.column === props.column ? drafts.open.mode : null;
  return {
    new: Math.max(0, total.new - (open === 'new' ? 1 : 0)),
    edit: Math.max(0, total.edit - (open === 'edit' ? 1 : 0)),
  };
});
const beingWritten = computed(() => writing.value.new + writing.value.edit);

/** The strip's mini cards: sent ones (edited ones shimmer), then new ones being written. */
const minis = computed(() => {
  const sent = Math.min(fromTeam.value, MAX_MINI_CARDS);
  const edited = Math.min(writing.value.edit, sent);
  const cards: { key: string; writing: boolean }[] = [];
  for (let i = 0; i < sent; i++) cards.push({ key: `s${i}`, writing: i >= sent - edited });
  const fresh = Math.min(writing.value.new, MAX_MINI_CARDS - sent);
  for (let i = 0; i < fresh; i++) cards.push({ key: `w${i}`, writing: true });
  return cards;
});

const strip = computed(() => {
  const sent = t('reflect.fromTeam', fromTeam.value);
  return beingWritten.value > 0
    ? `${sent} · ${t('reflect.beingWritten', { count: beingWritten.value })}`
    : sent;
});
</script>

<template>
  <section
    class="col"
    :aria-label="name"
    :style="{ '--fg': `var(--col-${column}-fg)`, '--bg': `var(--col-${column}-bg)` }"
  >
    <div class="col__head">
      <div class="col__title">
        <h2 class="col__name"><span class="col__dot" aria-hidden="true"></span>{{ name }}</h2>
        <span class="col__hint">{{ t(`reflect.columns.${column}.hint`) }}</span>
      </div>
      <span v-if="mine.length > 0" class="col__mine">
        {{ t('reflect.mine', { count: mine.length }) }}
      </span>
    </div>
    <div class="col__body">
      <ReflectionComposer v-if="composing" :column="column" />
      <button
        v-else
        type="button"
        class="col__write"
        :aria-label="t('reflect.writeIn', { column: name })"
        @click="drafts.startNew(column)"
      >
        <AppIcon name="pencil" :size="15" />{{ t('reflect.write') }}
      </button>
      <template v-for="reflection in mine" :key="reflection.id">
        <ReflectionComposer
          v-if="editingId === reflection.id"
          :column="column"
          :reflection="reflection"
        />
        <ReflectionCard v-else :reflection="reflection" />
      </template>
      <div class="col__team" data-testid="team-strip">
        <span class="col__count" aria-live="polite">
          <AppIcon name="eyeOff" :size="14" />{{ strip }}
        </span>
        <div v-if="minis.length > 0" class="col__minis" aria-hidden="true">
          <span
            v-for="card in minis"
            :key="card.key"
            class="col__mini"
            :class="{ 'col__mini--writing': card.writing }"
            :data-writing="card.writing || undefined"
          ></span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.col {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 420px;
  border: 1px solid var(--well-edge);
  border-radius: var(--radius-panel);
  background: var(--well);
}
.col__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 14px 14px 10px;
}
.col__title {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.col__name {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  color: var(--text);
  font-size: 17px;
  font-weight: 700;
}
.col__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--fg);
}
.col__hint {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.col__mine {
  display: inline-flex;
  align-items: center;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--bg);
  color: var(--fg);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.col__body {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 10px;
  padding: 0 12px 14px;
}
.col__write {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 44px;
  padding: 0 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--card);
  color: var(--muted);
  font: inherit;
  font-size: 14px;
  text-align: left;
  cursor: text;
}
.col__write:hover {
  color: var(--text);
}
.col__team {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: auto;
  padding-top: 12px;
  border-top: 1px dashed var(--line);
}
.col__count {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
  font-size: 12px;
}
.col__minis {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  padding: 2px;
  overflow: hidden;
}
.col__mini {
  flex: none;
  box-sizing: border-box;
  width: 34px;
  height: 22px;
  border-radius: 5px;
  background: var(--fg);
  box-shadow: 0 0 0 2px var(--well);
}
.col__mini + .col__mini {
  margin-left: -18px;
}
.col__mini--writing {
  border: 1px solid var(--fg);
  background: linear-gradient(90deg, var(--bg) 0%, var(--fg) 50%, var(--bg) 100%);
  background-size: 200% 100%;
  animation: retroWriting 1.6s linear infinite;
}
</style>
