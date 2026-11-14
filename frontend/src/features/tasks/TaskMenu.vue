<script setup lang="ts">
/**
 * The "Task actions" button of a card and its menu (spec §10.1): Edit, Move to…
 * (a submenu of the other columns) and Delete. Edit and Delete are for
 * members who may edit the task. Move works from the keyboard, as drag and
 * drop does not.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { TaskStatus, type TaskDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import { usePopover } from '@/composables/usePopover';
import { useTasksStore } from '@/stores/tasks';

const COLUMNS = [TaskStatus.future, TaskStatus.stuck, TaskStatus.active, TaskStatus.done] as const;

const props = defineProps<{ task: TaskDto }>();
const emit = defineEmits<{ edit: []; delete: []; moved: [status: TaskStatus] }>();
const { t } = useI18n();
const store = useTasksStore();

const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const menu = ref<HTMLElement | null>(null);
const submenu = ref<HTMLElement | null>(null);
const moveItem = ref<HTMLButtonElement | null>(null);
const { open, close } = usePopover(root, trigger);
const moving = ref(false);

const targets = computed(() => COLUMNS.filter((status) => status !== props.task.status));

const PANEL_WIDTH = 200;
const EDGE = 8;
const GAP = 4;
/** One menu row: 44 px plus the 2 px gap. */
const ROW = 46;
/**
 * The panel is `position: fixed`, so the board's scroll box can never clip it:
 * it hangs below the button, or above it when there is no room below (with
 * the Move to… submenu counted, so opening it does not push the panel off screen).
 */
const placement = ref<Record<string, string>>({});

function place(): void {
  const button = trigger.value;
  const panel = menu.value;
  if (!button || !panel) return;
  const rect = button.getBoundingClientRect();
  const left = Math.max(
    EDGE,
    Math.min(rect.right + 12 - PANEL_WIDTH, window.innerWidth - PANEL_WIDTH - EDGE),
  );
  const needed = panel.offsetHeight + (moving.value ? 0 : targets.value.length * ROW);
  const below = window.innerHeight - rect.bottom - GAP - EDGE;
  const above = rect.top - GAP - EDGE;
  placement.value =
    needed <= below || below >= above
      ? { left: `${left}px`, top: `${rect.bottom - GAP}px` }
      : { left: `${left}px`, bottom: `${window.innerHeight - rect.top + GAP}px` };
}

/** The anchor moves when the page or the board scrolls: close rather than chase it. */
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
watch(moving, async () => {
  await nextTick();
  place();
});

async function toggle(): Promise<void> {
  moving.value = false;
  open.value = !open.value;
  if (!open.value) return;
  await nextTick();
  place();
  focusItem(menu.value, 0);
}

function itemsOf(container: HTMLElement | null): HTMLElement[] {
  if (!container) return [];
  return [...container.querySelectorAll<HTMLElement>(':scope > [role="menuitem"]')];
}

function focusItem(container: HTMLElement | null, index: number): void {
  const list = itemsOf(container);
  if (list.length === 0) return;
  // preventScroll: the panel is fixed, and a scroll would close it.
  list[(index + list.length) % list.length]?.focus({ preventScroll: true });
}

/** Arrow keys, Home and End cycle through the menu the focus is in; true when handled. */
function navigate(event: KeyboardEvent, current: HTMLElement | null): boolean {
  const list = itemsOf(current);
  const index = list.indexOf(document.activeElement as HTMLElement);
  switch (event.key) {
    case 'ArrowDown':
      focusItem(current, index + 1);
      break;
    case 'ArrowUp':
      focusItem(current, index < 0 ? -1 : index - 1);
      break;
    case 'Home':
      focusItem(current, 0);
      break;
    case 'End':
      focusItem(current, -1);
      break;
    default:
      return false;
  }
  event.preventDefault();
  return true;
}

function onKeyDown(event: KeyboardEvent): void {
  navigate(event, menu.value);
}

async function openMove(): Promise<void> {
  moving.value = true;
  await nextTick();
  focusItem(submenu.value, 0);
}

function closeMove(): void {
  moving.value = false;
  moveItem.value?.focus();
}

/** ArrowRight opens the submenu; ArrowDown enters it when it is already open. */
function onMoveKeyDown(event: KeyboardEvent): void {
  if (event.key === 'ArrowRight' || (event.key === 'ArrowDown' && moving.value)) {
    event.preventDefault();
    // The main menu's own handler must not move on to the next item.
    event.stopPropagation();
    void openMove();
  }
}

/** Escape and ArrowLeft in the submenu leave it for the main menu; Escape stops there. */
function onSubmenuKeyDown(event: KeyboardEvent): void {
  if (event.key === 'Escape' || event.key === 'ArrowLeft') {
    event.preventDefault();
    event.stopPropagation();
    closeMove();
    return;
  }
  // Handled keys stop here: the main menu would otherwise navigate as well.
  if (navigate(event, submenu.value)) event.stopPropagation();
}

function onFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget as Node | null;
  if (next && root.value && !root.value.contains(next)) close();
}

function moveTo(status: TaskStatus): void {
  close(true);
  // Appended: the column's length counts the cards already there.
  void store.move(props.task.id, status, store.columns[status].length);
  emit('moved', status);
}

function act(action: 'edit' | 'delete'): void {
  close(true);
  if (action === 'edit') emit('edit');
  else emit('delete');
}
</script>

<template>
  <div ref="root" class="menu" @focusout="onFocusOut">
    <button
      ref="trigger"
      type="button"
      class="menu__trigger"
      :aria-label="t('tasks.actions')"
      aria-haspopup="menu"
      :aria-expanded="open"
      @click="toggle"
    >
      <AppIcon name="moreVertical" :size="18" />
    </button>

    <div
      v-if="open"
      ref="menu"
      class="menu__panel"
      role="menu"
      :aria-label="t('tasks.actions')"
      :style="{ position: 'fixed', ...placement }"
      @keydown="onKeyDown"
    >
      <button v-if="task.canEdit" type="button" role="menuitem" class="item" @click="act('edit')">
        <AppIcon name="pencil" :size="16" />
        {{ t('tasks.edit') }}
      </button>
      <button
        ref="moveItem"
        type="button"
        role="menuitem"
        class="item"
        aria-haspopup="menu"
        :aria-expanded="moving"
        @click="moving ? closeMove() : openMove()"
        @keydown="onMoveKeyDown"
      >
        <AppIcon name="move" :size="16" />
        <span class="item__label">{{ t('tasks.moveTo') }}</span>
        <AppIcon name="chevronRight" :size="14" />
      </button>
      <div
        v-if="moving"
        ref="submenu"
        class="submenu"
        role="menu"
        :aria-label="t('tasks.moveTo')"
        @keydown="onSubmenuKeyDown"
      >
        <button
          v-for="status in targets"
          :key="status"
          type="button"
          role="menuitem"
          class="item"
          @click="moveTo(status)"
        >
          {{ t(`tasks.columns.${status}`) }}
        </button>
      </div>
      <template v-if="task.canEdit">
        <div class="menu__separator" role="separator" />
        <button type="button" role="menuitem" class="item item--danger" @click="act('delete')">
          <AppIcon name="trash" :size="16" />
          {{ t('tasks.delete') }}
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.menu {
  position: relative;
  flex: none;
}
.menu__trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  /* 44 px hit area without growing the card: the glyph box stays 28 px. */
  width: 44px;
  height: 44px;
  margin: -8px -10px -8px 0;
  padding: 0;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.menu__trigger:hover,
.menu__trigger[aria-expanded='true'] {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.menu__panel {
  /* position, top/bottom and left are set inline from the button's rect. */
  z-index: 10;
  display: flex;
  flex-direction: column;
  gap: 2px;
  box-sizing: border-box;
  width: 200px;
  padding: 6px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 14px;
  box-shadow: var(--shadow-menu);
  cursor: default;
}
.menu__separator {
  height: 1px;
  margin: 4px 6px;
  background: var(--line);
}
.submenu {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-left: 12px;
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
.item--danger {
  color: var(--danger);
}
.item__label {
  flex: 1 1 auto;
}
</style>
