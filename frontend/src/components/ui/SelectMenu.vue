<script setup lang="ts">
/**
 * A popover listbox opened by a button: single or multiple choice, optionally
 * with a search field. The `trigger` slot renders the button; bind its `attrs`
 * on it. Esc or a press outside closes the list; a single choice closes it too.
 */
import { computed, nextTick, ref, useId, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import type { SelectOption } from '@/components/ui/select';
import { usePopover } from '@/composables/usePopover';

const props = withDefaults(
  defineProps<{
    options: SelectOption[];
    /** The chosen value, or the chosen values when `multiple`. */
    modelValue: string | string[];
    /** Accessible name of the list. */
    label: string;
    multiple?: boolean;
    /** Placeholder of the search field; the field is shown only when set. */
    searchPlaceholder?: string;
    /** How many leading options stay visible while searching, set apart by a rule. */
    pinned?: number;
    /** Panel width in px (capped by the viewport). */
    width?: number;
  }>(),
  { multiple: false, searchPlaceholder: undefined, pinned: 0, width: 300 },
);
const emit = defineEmits<{ 'update:modelValue': [value: string | string[]] }>();
const { t } = useI18n();

const uid = useId();
const listId = `${uid}-list`;
const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLElement | null>(null);
const panel = ref<HTMLElement | null>(null);
const listbox = ref<HTMLElement | null>(null);
const search = ref<HTMLInputElement | null>(null);
const { open, close } = usePopover(root, trigger);

const query = ref('');
const active = ref(0);
/** Horizontal nudge that keeps the panel inside the viewport. */
const shift = ref(0);

const selected = computed(
  () => new Set(Array.isArray(props.modelValue) ? props.modelValue : [props.modelValue]),
);

function normalize(text: string): string {
  return text
    .toLocaleLowerCase()
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');
}

const visible = computed(() => {
  const q = normalize(query.value.trim());
  const head = props.options.slice(0, props.pinned);
  const rest = props.options
    .slice(props.pinned)
    .filter((option) => !q || normalize(option.label).includes(q));
  return [...head, ...rest];
});
const noMatches = computed(() => !!query.value.trim() && visible.value.length <= props.pinned);

function optionId(index: number): string {
  return `${uid}-option-${index}`;
}
const activeId = computed(() => (visible.value[active.value] ? optionId(active.value) : undefined));

function openMenu(): void {
  trigger.value = root.value?.querySelector<HTMLElement>('[aria-haspopup="listbox"]') ?? null;
  query.value = '';
  active.value = Math.max(
    visible.value.findIndex((option) => selected.value.has(option.value)),
    0,
  );
  shift.value = 0;
  open.value = true;
}

function toggle(): void {
  if (open.value) close();
  else openMenu();
}

function onTriggerKey(event: KeyboardEvent): void {
  if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && !open.value) {
    event.preventDefault();
    openMenu();
  }
}

const triggerAttrs = computed(() => ({
  'aria-haspopup': 'listbox' as const,
  'aria-expanded': open.value,
  'aria-controls': open.value ? listId : undefined,
  onClick: toggle,
  onKeydown: onTriggerKey,
}));

watch(open, async (isOpen) => {
  if (!isOpen) return;
  await nextTick();
  // Pull the panel back inside the viewport first: focusing it while it sticks
  // out would scroll the whole page sideways.
  const rect = panel.value?.getBoundingClientRect();
  if (rect) shift.value = Math.min(0, document.documentElement.clientWidth - 16 - rect.right);
  await nextTick();
  (search.value ?? listbox.value)?.focus({ preventScroll: true });
  scrollToActive();
});
watch(query, () => {
  active.value = 0;
});

function scrollToActive(): void {
  const el = listbox.value?.querySelectorAll<HTMLElement>('[role="option"]')[active.value];
  el?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
}

function setActive(index: number): void {
  active.value = index;
  void nextTick(scrollToActive);
}

function pick(option: SelectOption): void {
  if (props.multiple) {
    const values = Array.isArray(props.modelValue) ? props.modelValue : [];
    emit(
      'update:modelValue',
      values.includes(option.value)
        ? values.filter((v) => v !== option.value)
        : [...values, option.value],
    );
  } else {
    emit('update:modelValue', option.value);
    close(true);
  }
}

function onListKey(event: KeyboardEvent): void {
  const count = visible.value.length;
  const inSearch = event.target === search.value;
  const current = visible.value[active.value];
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault();
      if (count) setActive((active.value + 1) % count);
      break;
    case 'ArrowUp':
      event.preventDefault();
      if (count) setActive((active.value - 1 + count) % count);
      break;
    case 'Home':
    case 'End':
      if (inSearch || !count) return;
      event.preventDefault();
      setActive(event.key === 'Home' ? 0 : count - 1);
      break;
    case 'Enter':
      event.preventDefault();
      if (current) pick(current);
      break;
    case ' ':
      if (inSearch) return;
      event.preventDefault();
      if (current) pick(current);
      break;
  }
}

/** Tabbing away closes the list. */
function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (open.value && next && !root.value?.contains(next)) close();
}
</script>

<template>
  <div ref="root" class="select-menu" @focusout="onFocusOut">
    <slot name="trigger" :attrs="triggerAttrs" :open="open" />
    <div v-if="open" ref="panel" class="panel" :style="{ width: `${width}px`, left: `${shift}px` }">
      <div v-if="searchPlaceholder" class="search">
        <AppIcon name="search" :size="16" class="search__icon" />
        <input
          ref="search"
          v-model="query"
          type="text"
          class="search__input"
          role="combobox"
          autocomplete="off"
          aria-autocomplete="list"
          aria-expanded="true"
          :aria-controls="listId"
          :aria-activedescendant="activeId"
          :aria-label="searchPlaceholder"
          :placeholder="searchPlaceholder"
          @keydown="onListKey"
        />
      </div>
      <div
        :id="listId"
        ref="listbox"
        role="listbox"
        class="list"
        :aria-label="label"
        :aria-multiselectable="multiple || undefined"
        :tabindex="searchPlaceholder ? -1 : 0"
        :aria-activedescendant="searchPlaceholder ? undefined : activeId"
        @keydown="onListKey"
      >
        <template v-for="(option, i) in visible" :key="option.value">
          <div
            :id="optionId(i)"
            role="option"
            class="option"
            :class="{
              'option--active': i === active,
              'option--selected': selected.has(option.value),
            }"
            :aria-selected="selected.has(option.value)"
            @mousedown.prevent
            @mousemove="active = i"
            @click="pick(option)"
          >
            <span v-if="multiple" class="option__box" aria-hidden="true">
              <AppIcon
                v-if="selected.has(option.value)"
                name="check"
                :size="12"
                :stroke-width="3.5"
              />
            </span>
            <slot name="leading" :option="option" />
            <span class="option__text">
              <span class="option__label">{{ option.label }}</span>
              <span v-if="option.hint" class="option__hint">{{ option.hint }}</span>
            </span>
            <AppIcon
              v-if="!multiple && selected.has(option.value)"
              name="check"
              :size="16"
              :stroke-width="2.5"
              class="option__check"
            />
          </div>
          <div v-if="i === pinned - 1 && i < visible.length - 1" class="rule" aria-hidden="true" />
        </template>
      </div>
      <p v-if="noMatches" class="empty">{{ t('common.noMatches') }}</p>
      <div v-if="$slots.footer" class="footer">
        <slot name="footer" :close="() => close(true)" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.select-menu {
  position: relative;
  display: inline-flex;
  max-width: 100%;
}
.panel {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  z-index: 20;
  box-sizing: border-box;
  max-width: calc(100vw - 32px);
  padding: 6px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-menu);
  color: var(--text);
}
.search {
  position: relative;
  padding: 4px 4px 6px;
}
.search__icon {
  position: absolute;
  top: 16px;
  left: 16px;
  color: var(--muted);
  pointer-events: none;
}
.search__input {
  box-sizing: border-box;
  width: 100%;
  height: 40px;
  padding: 0 12px 0 36px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--line);
  background: var(--well);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
}
.search__input::placeholder {
  color: var(--muted);
}
.search__input:focus-visible {
  outline: none;
  border-color: var(--accent);
}
.list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 320px;
  overflow-y: auto;
}
.list:focus-visible {
  outline: none;
}
.option {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 6px 10px;
  box-sizing: border-box;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.option--active {
  background: var(--accent-soft);
}
.option__box {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 18px;
  height: 18px;
  box-sizing: border-box;
  border-radius: 5px;
  border: 2px solid var(--muted);
  color: var(--on-accent);
}
.option--selected .option__box {
  border-color: var(--accent);
  background: var(--accent);
}
.option__text {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.option__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  font-weight: 500;
}
.option--selected .option__label {
  font-weight: 600;
}
.option__hint {
  font-size: 12px;
  color: var(--muted);
}
.option__check {
  color: var(--accent-text);
}
.rule {
  height: 1px;
  margin: 4px 6px;
  background: var(--line);
}
.empty {
  margin: 0;
  padding: 10px;
  font-size: 13px;
  color: var(--muted);
}
.footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: 4px;
  padding: 8px 6px 4px;
  border-top: 1px solid var(--line);
}
</style>
