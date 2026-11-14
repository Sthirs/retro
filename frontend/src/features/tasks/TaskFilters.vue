<script setup lang="ts">
/** The board's filter row (spec §10): My tasks, Mentioned, Author, Meeting. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { MemberDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import type { TaskFilter } from '@/stores/tasks';

const props = defineProps<{
  filter: TaskFilter;
  members: MemberDto[];
  membersLoaded: boolean;
  meetingIds: string[];
  meId: string | null;
}>();
const emit = defineEmits<{ change: [filter: TaskFilter] }>();
const { t } = useI18n();

const myTasks = computed(() => !!props.meId && props.filter.mentioned === props.meId);
const active = computed(() => Object.values(props.filter).some(Boolean));
// A meeting stays selectable while it is the active filter, even if it matches no task.
const meetings = computed(() => {
  const ids = new Set(props.meetingIds);
  if (props.filter.meeting) ids.add(props.filter.meeting);
  return [...ids];
});

/**
 * The people to pick from, plus the current URL value when it is not among them, so the select
 * never claims "anyone" while filtering. It is "unknown" only once the member list has loaded.
 */
function people(current: string | undefined): { id: string; name: string }[] {
  const list = props.members.map((m) => ({ id: m.user.id, name: m.user.displayName }));
  if (current && !list.some((p) => p.id === current)) {
    const name = props.membersLoaded ? 'tasks.filters.unknown' : 'tasks.filters.selectedMember';
    list.push({ id: current, name: t(name) });
  }
  return list;
}

type Key = keyof TaskFilter;
function set(key: Key, value: string): void {
  emit('change', { ...props.filter, [key]: value || undefined });
}
function onSelect(key: Key, event: Event): void {
  set(key, (event.target as HTMLSelectElement).value);
}
function toggleMine(): void {
  set('mentioned', myTasks.value || !props.meId ? '' : props.meId);
}
</script>

<template>
  <div class="filters" role="group" :aria-label="t('tasks.filters.label')">
    <AppIcon name="filter" :size="17" class="filters__icon" />

    <button
      type="button"
      class="chip chip--toggle"
      :class="{ 'chip--on': myTasks }"
      :aria-pressed="myTasks"
      :disabled="!meId"
      @click="toggleMine"
    >
      <AppIcon name="atSign" :size="15" />
      {{ t('tasks.filters.myTasks') }}
    </button>

    <label class="chip chip--select" :class="{ 'chip--on': !!filter.mentioned }">
      <span aria-hidden="true">{{ t('tasks.filters.mentioned') }}:</span>
      <select
        class="chip__select"
        :aria-label="t('tasks.filters.mentioned')"
        :value="filter.mentioned ?? ''"
        @change="onSelect('mentioned', $event)"
      >
        <option value="">{{ t('tasks.filters.anyone') }}</option>
        <option v-if="meId && filter.mentioned === meId" :value="meId">
          {{ t('tasks.filters.me') }}
        </option>
        <template v-for="p in people(filter.mentioned)" :key="p.id">
          <option v-if="!(meId && filter.mentioned === meId && p.id === meId)" :value="p.id">
            {{ p.name }}
          </option>
        </template>
      </select>
      <AppIcon name="chevronDown" :size="14" class="chip__chevron" />
    </label>

    <label class="chip chip--select" :class="{ 'chip--on': !!filter.author }">
      <span aria-hidden="true">{{ t('tasks.filters.author') }}:</span>
      <select
        class="chip__select"
        :aria-label="t('tasks.filters.author')"
        :value="filter.author ?? ''"
        @change="onSelect('author', $event)"
      >
        <option value="">{{ t('tasks.filters.anyone') }}</option>
        <option v-for="p in people(filter.author)" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <AppIcon name="chevronDown" :size="14" class="chip__chevron" />
    </label>

    <label
      v-if="meetings.length"
      class="chip chip--select"
      :class="{ 'chip--on': !!filter.meeting }"
    >
      <span aria-hidden="true">{{ t('tasks.filters.meeting') }}:</span>
      <select
        class="chip__select"
        :aria-label="t('tasks.filters.meeting')"
        :value="filter.meeting ?? ''"
        @change="onSelect('meeting', $event)"
      >
        <option value="">{{ t('tasks.filters.all') }}</option>
        <option v-for="(id, i) in meetings" :key="id" :value="id">
          {{ t('tasks.filters.meetingN', { n: i + 1 }) }}
        </option>
      </select>
      <AppIcon name="chevronDown" :size="14" class="chip__chevron" />
    </label>

    <button v-if="active" type="button" class="clear" @click="emit('change', {})">
      {{ t('tasks.filters.clear') }}
    </button>
  </div>
</template>

<style scoped>
.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.filters__icon {
  color: var(--muted);
}
.chip {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 14px;
  border-radius: 999px;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
}
.chip--toggle {
  cursor: pointer;
}
.chip:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}
.chip--on {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent-text);
}
.chip--select:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.chip__select {
  appearance: none;
  min-height: 42px;
  /* Room on the right for the chevron. */
  padding: 0 20px 0 0;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}
.chip__select:focus-visible {
  outline: none;
}
.chip__select option {
  background: var(--surface);
  color: var(--text);
}
.chip__chevron {
  position: absolute;
  right: 12px;
  pointer-events: none;
}
.clear {
  min-height: 44px;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--accent-text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.clear:hover {
  color: var(--text);
}
</style>
