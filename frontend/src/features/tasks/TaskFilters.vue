<script setup lang="ts">
/**
 * The board's filter row (spec §10): My tasks, Mentioned (several people),
 * Author and Meeting, each a dropdown chip that a ✕ clears.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { MemberDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import SelectMenu from '@/components/ui/SelectMenu.vue';
import type { SelectOption } from '@/components/ui/select';
import { joinIds, splitIds, type TaskFilter } from '@/stores/tasks';

const props = defineProps<{
  filter: TaskFilter;
  members: MemberDto[];
  membersLoaded: boolean;
  meetingIds: string[];
  meId: string | null;
}>();
const emit = defineEmits<{ change: [filter: TaskFilter] }>();
const { t } = useI18n();

const mentioned = computed(() => splitIds(props.filter.mentioned));
const myTasks = computed(
  () => !!props.meId && mentioned.value.length === 1 && mentioned.value[0] === props.meId,
);
const active = computed(() => Object.values(props.filter).some(Boolean));
// A meeting stays selectable while it is the active filter, even if it matches no task.
const meetings = computed(() => {
  const ids = new Set(props.meetingIds);
  if (props.filter.meeting) ids.add(props.filter.meeting);
  return [...ids];
});

interface Person {
  id: string;
  name: string;
  pictureVersion?: string | null;
}

/**
 * The people to pick from, the current user first, plus any filtered id that is
 * not among them, so a chip never claims "anyone" while filtering. Such an id is
 * "unknown" only once the member list has loaded.
 */
function people(current: string[]): Person[] {
  const list: Person[] = props.members.map((m) => ({
    id: m.user.id,
    name: m.user.displayName,
    pictureVersion: m.user.pictureVersion,
  }));
  list.sort((a, b) => Number(b.id === props.meId) - Number(a.id === props.meId));
  for (const id of current) {
    if (!list.some((p) => p.id === id)) {
      const name = props.membersLoaded ? 'tasks.filters.unknown' : 'tasks.filters.selectedMember';
      list.push({ id, name: t(name) });
    }
  }
  return list;
}

function personOptions(list: Person[]): SelectOption[] {
  return list.map((p) => ({
    value: p.id,
    label: p.id === props.meId ? t('tasks.filters.you', { name: p.name }) : p.name,
  }));
}

const mentionedPeople = computed(() => people(mentioned.value));
const authorPeople = computed(() => people(props.filter.author ? [props.filter.author] : []));
const byId = computed(
  () => new Map([...mentionedPeople.value, ...authorPeople.value].map((p) => [p.id, p])),
);

const mentionedOptions = computed(() => personOptions(mentionedPeople.value));
const authorOptions = computed<SelectOption[]>(() => [
  { value: '', label: t('tasks.filters.anyoneOption') },
  ...personOptions(authorPeople.value),
]);
const meetingOptions = computed<SelectOption[]>(() => [
  { value: '', label: t('tasks.filters.allMeetings') },
  ...meetings.value.map((id, i) => ({
    value: id,
    label: t('tasks.filters.meetingN', { n: i + 1 }),
  })),
]);

/**
 * A person's short name for a chip: "me", the first word of a member's display
 * name, or the whole placeholder for someone who is not (or not yet) a member.
 */
function shortName(id: string): string {
  if (id === props.meId) return t('tasks.filters.me');
  const name = byId.value.get(id)?.name ?? t('tasks.filters.selectedMember');
  const isMember = props.members.some((m) => m.user.id === id);
  return isMember ? name.split(/\s+/)[0] || name : name;
}

const mentionedValue = computed(() =>
  mentioned.value.length ? mentioned.value.map(shortName).join(', ') : '',
);
const authorValue = computed(() => (props.filter.author ? shortName(props.filter.author) : ''));
const meetingValue = computed(() =>
  props.filter.meeting
    ? (meetingOptions.value.find((o) => o.value === props.filter.meeting)?.label ?? '')
    : '',
);

type Key = keyof TaskFilter;
function set(key: Key, value: string): void {
  emit('change', { ...props.filter, [key]: value || undefined });
}
function setMentioned(value: string | string[]): void {
  set('mentioned', joinIds(Array.isArray(value) ? value : [value]));
}
function setOne(key: Key, value: string | string[]): void {
  set(key, Array.isArray(value) ? (value[0] ?? '') : value);
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
      class="toggle"
      :class="{ 'toggle--on': myTasks }"
      :aria-pressed="myTasks"
      :disabled="!meId"
      @click="toggleMine"
    >
      <AppIcon name="atSign" :size="15" />
      {{ t('tasks.filters.myTasks') }}
    </button>

    <SelectMenu
      :model-value="mentioned"
      :options="mentionedOptions"
      :label="t('tasks.filters.mentioned')"
      :search-placeholder="t('tasks.filters.searchPeople')"
      multiple
      data-testid="filter-mentioned"
      @update:model-value="setMentioned"
    >
      <template #trigger="{ attrs, open }">
        <span class="chip" :class="{ 'chip--on': !!mentionedValue, 'chip--open': open }">
          <button type="button" class="chip__trigger" v-bind="attrs">
            <span class="chip__text">
              {{ t('tasks.filters.mentioned') }}:
              <strong v-if="mentionedValue" class="chip__value">{{ mentionedValue }}</strong>
              <template v-else>{{ t('tasks.filters.anyone') }}</template>
            </span>
            <AppIcon
              name="chevronDown"
              :size="14"
              class="chip__chevron"
              :class="{ 'chip__chevron--open': open }"
            />
          </button>
          <button
            v-if="mentionedValue"
            type="button"
            class="chip__clear"
            :aria-label="t('tasks.filters.clearOne', { name: t('tasks.filters.mentioned') })"
            @click="set('mentioned', '')"
          >
            <AppIcon name="close" :size="14" />
          </button>
        </span>
      </template>
      <template #leading="{ option }">
        <UserAvatar
          :user-id="option.value"
          :name="byId.get(option.value)?.name ?? option.label"
          :picture-version="byId.get(option.value)?.pictureVersion ?? null"
          :size="28"
        />
      </template>
      <template #footer="{ close }">
        <button type="button" class="footer__clear" @click="set('mentioned', '')">
          {{ t('tasks.filters.clearShort') }}
        </button>
        <AppButton variant="primary" size="compact" @click="close">
          {{ t('tasks.filters.done') }}
        </AppButton>
      </template>
    </SelectMenu>

    <SelectMenu
      :model-value="filter.author ?? ''"
      :options="authorOptions"
      :label="t('tasks.filters.author')"
      :search-placeholder="t('tasks.filters.searchPeople')"
      :pinned="1"
      data-testid="filter-author"
      @update:model-value="setOne('author', $event)"
    >
      <template #trigger="{ attrs, open }">
        <span class="chip" :class="{ 'chip--on': !!authorValue, 'chip--open': open }">
          <button type="button" class="chip__trigger" v-bind="attrs">
            <span class="chip__text">
              {{ t('tasks.filters.author') }}:
              <strong v-if="authorValue" class="chip__value">{{ authorValue }}</strong>
              <template v-else>{{ t('tasks.filters.anyone') }}</template>
            </span>
            <AppIcon
              name="chevronDown"
              :size="14"
              class="chip__chevron"
              :class="{ 'chip__chevron--open': open }"
            />
          </button>
          <button
            v-if="authorValue"
            type="button"
            class="chip__clear"
            :aria-label="t('tasks.filters.clearOne', { name: t('tasks.filters.author') })"
            @click="set('author', '')"
          >
            <AppIcon name="close" :size="14" />
          </button>
        </span>
      </template>
      <template #leading="{ option }">
        <span v-if="!option.value" class="icon-tile"><AppIcon name="users" :size="15" /></span>
        <UserAvatar
          v-else
          :user-id="option.value"
          :name="byId.get(option.value)?.name ?? option.label"
          :picture-version="byId.get(option.value)?.pictureVersion ?? null"
          :size="28"
        />
      </template>
    </SelectMenu>

    <SelectMenu
      v-if="meetings.length"
      :model-value="filter.meeting ?? ''"
      :options="meetingOptions"
      :label="t('tasks.filters.meeting')"
      :search-placeholder="t('tasks.filters.searchMeetings')"
      :pinned="1"
      :width="320"
      data-testid="filter-meeting"
      @update:model-value="setOne('meeting', $event)"
    >
      <template #trigger="{ attrs, open }">
        <span class="chip" :class="{ 'chip--on': !!meetingValue, 'chip--open': open }">
          <button type="button" class="chip__trigger" v-bind="attrs">
            <span class="chip__text">
              {{ t('tasks.filters.meeting') }}:
              <strong v-if="meetingValue" class="chip__value">{{ meetingValue }}</strong>
              <template v-else>{{ t('tasks.filters.all') }}</template>
            </span>
            <AppIcon
              name="chevronDown"
              :size="14"
              class="chip__chevron"
              :class="{ 'chip__chevron--open': open }"
            />
          </button>
          <button
            v-if="meetingValue"
            type="button"
            class="chip__clear"
            :aria-label="t('tasks.filters.clearOne', { name: t('tasks.filters.meeting') })"
            @click="set('meeting', '')"
          >
            <AppIcon name="close" :size="14" />
          </button>
        </span>
      </template>
      <template #leading="{ option }">
        <span class="icon-tile">
          <AppIcon :name="option.value ? 'calendar' : 'list'" :size="15" />
        </span>
      </template>
    </SelectMenu>

    <button v-if="active" type="button" class="clear" @click="emit('change', {})">
      {{ t('tasks.filters.clear') }}
    </button>
  </div>
</template>

<style scoped>
.filters {
  position: relative;
  z-index: 3;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.filters__icon {
  color: var(--muted);
}
.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 14px;
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.toggle:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}
.toggle--on {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent-text);
}
.chip {
  display: inline-flex;
  align-items: center;
  max-width: 100%;
  border-radius: var(--radius-pill);
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--text);
}
.chip--on {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent-text);
}
.chip--open {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}
.chip__trigger {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  min-height: 42px;
  padding: 0 14px;
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
}
.chip--on .chip__trigger {
  padding-right: 4px;
}
.chip__trigger:focus-visible,
.chip__clear:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.chip__text {
  min-width: 0;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.chip__value {
  font-weight: 700;
}
.chip__chevron {
  transition: transform 0.15s ease;
}
.chip__chevron--open {
  transform: rotate(180deg);
}
.chip__clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  min-height: 42px;
  margin-right: 4px;
  padding: 0;
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.icon-tile {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  background: var(--well);
  color: var(--muted);
}
.footer__clear {
  min-height: 36px;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--muted);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.footer__clear:hover {
  color: var(--text);
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
@media (prefers-reduced-motion: reduce) {
  .chip__chevron {
    transition: none;
  }
}
</style>
