<script setup lang="ts">
/**
 * The emoji picker of a card's reactions (ReactionPicker artboard). First
 * row: the viewer's most used emoji, kept in this browser (spec §9.5). Then
 * a search, the category tabs (Recently used, then the Unicode groups) and
 * the emoji, and a preview of the one under the pointer or focus. Picking an
 * emoji reacts (or takes the reaction back) and closes the picker; Esc
 * closes it. Arrow keys move between emoji.
 */
import { computed, nextTick, onMounted, ref, useId } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import {
  CATEGORY_IDS,
  type CategoryId,
  type EmojiCategory,
  type EmojiEntry,
  loadEmojis,
  searchEmojis,
} from '@/features/meetings/discuss/emojiData';
import { mostUsed, readUsage } from '@/features/meetings/discuss/emojiUsage';
import { useSessionStore } from '@/stores/session';

const props = defineProps<{
  /** The viewer's own emoji on the card: shown pressed. */
  mine: string[];
}>();
const emit = defineEmits<{ pick: [emoji: string]; close: [] }>();
const { t } = useI18n();
const session = useSessionStore();
const uid = useId();

const usage = readUsage(session.user?.id ?? '');
const quick = mostUsed(usage);
const recent = usage.recent;

const categories = ref<EmojiCategory[]>([]);
const loadState = ref<'loading' | 'ready' | 'failed'>('loading');
const query = ref('');
const search = ref<HTMLInputElement | null>(null);
const grid = ref<HTMLElement | null>(null);
type TabId = 'recent' | CategoryId;
const activeTab = ref<TabId>('recent');

const byEmoji = computed(() => {
  const map = new Map<string, EmojiEntry>();
  for (const c of categories.value) for (const e of c.emojis) map.set(e.emoji, e);
  return map;
});
const entryOf = (emoji: string): EmojiEntry =>
  byEmoji.value.get(emoji) ?? { emoji, name: emoji, slug: '' };

const results = computed(() => searchEmojis(categories.value, query.value));
const searching = computed(() => query.value.trim().length > 0);

/** The sections the grid shows, in order. */
const sections = computed<{ id: TabId; emojis: EmojiEntry[] }[]>(() => [
  { id: 'recent', emojis: recent.map(entryOf) },
  ...categories.value.map((c) => ({ id: c.id, emojis: c.emojis })),
]);

/** The emoji the footer describes: under the pointer or focus, else the first quick one. */
const previewed = ref<string>(quick[0] ?? '👍');
const preview = computed(() => entryOf(previewed.value));

const isMine = (emoji: string) => props.mine.includes(emoji);

function pick(emoji: string): void {
  emit('pick', emoji);
}

/** Enter in the search reacts with the first result. */
function pickFirst(): void {
  const first = results.value[0];
  if (first) pick(first.emoji);
}

function showTab(id: TabId): void {
  activeTab.value = id;
  query.value = '';
  void nextTick(() => {
    const section = grid.value?.querySelector<HTMLElement>(`[data-section="${id}"]`);
    // The grid is positioned: its sections' offsets are from its top.
    if (section && grid.value) grid.value.scrollTop = section.offsetTop;
  });
}

/** Marks the tab of the section scrolled to. */
function onScroll(): void {
  if (!grid.value || searching.value) return;
  const top = grid.value.scrollTop + 8;
  let current: TabId = 'recent';
  for (const el of grid.value.querySelectorAll<HTMLElement>('[data-section]')) {
    if (el.offsetTop <= top) current = el.dataset.section as TabId;
  }
  activeTab.value = current;
}

/** Arrow keys between the emoji of the grid (8 per row). */
function onGridKey(event: KeyboardEvent): void {
  const steps: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -8, ArrowDown: 8 };
  const step = steps[event.key];
  if (step === undefined || !grid.value) return;
  const buttons = [...grid.value.querySelectorAll<HTMLButtonElement>('button[data-emoji]')];
  const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
  if (at < 0) return;
  event.preventDefault();
  const next = buttons[Math.min(buttons.length - 1, Math.max(0, at + step))];
  next?.focus();
}

/** The picker itself, kept on screen. */
const dialog = ref<HTMLElement | null>(null);
/** Moves the picker left so it does not run off a narrow screen. */
const shift = ref(0);
/** Room kept free at the screen's edges. */
const EDGE = 16;

/** Keeps the picker within the screen, and scrolls it into view. */
function placeOnScreen(): void {
  const el = dialog.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const over = rect.right - (window.innerWidth - EDGE);
  if (over > 0) shift.value = -Math.min(over, Math.max(0, rect.left - EDGE));
  el.scrollIntoView?.({ block: 'nearest' });
}

onMounted(async () => {
  placeOnScreen();
  search.value?.focus({ preventScroll: true });
  try {
    categories.value = await loadEmojis();
    loadState.value = 'ready';
    await nextTick();
    placeOnScreen();
  } catch {
    loadState.value = 'failed';
  }
});
</script>

<template>
  <div
    ref="dialog"
    role="dialog"
    class="picker"
    :style="shift ? { transform: `translateX(${shift}px)` } : undefined"
    :aria-label="t('discuss.picker.title')"
    data-testid="emoji-picker"
    @keydown.esc.stop.prevent="emit('close')"
  >
    <div class="picker__quick">
      <span :id="`${uid}-quick`" class="picker__label">{{ t('discuss.picker.quick') }}</span>
      <div class="picker__quick-row" role="group" :aria-labelledby="`${uid}-quick`">
        <button
          v-for="emoji in quick"
          :key="emoji"
          type="button"
          class="picker__quick-emoji"
          :aria-label="entryOf(emoji).name"
          :aria-pressed="isMine(emoji)"
          data-testid="quick-reaction"
          @mouseenter="previewed = emoji"
          @focus="previewed = emoji"
          @click="pick(emoji)"
        >
          {{ emoji }}
        </button>
      </div>
    </div>
    <div class="picker__search">
      <span class="picker__search-icon" aria-hidden="true"
        ><AppIcon name="search" :size="16"
      /></span>
      <input
        ref="search"
        v-model="query"
        type="text"
        class="picker__input"
        :aria-label="t('discuss.picker.search')"
        :placeholder="t('discuss.picker.search')"
        @keydown.enter.prevent="pickFirst"
      />
    </div>
    <div role="tablist" class="picker__tabs" :aria-label="t('discuss.picker.categories')">
      <button
        v-for="id in ['recent', ...CATEGORY_IDS] as TabId[]"
        :key="id"
        type="button"
        role="tab"
        class="picker__tab"
        :aria-selected="!searching && activeTab === id"
        :aria-label="t(`discuss.picker.${id}`)"
        :title="t(`discuss.picker.${id}`)"
        @click="showTab(id)"
      >
        {{
          {
            recent: '🕘',
            smileys: '😀',
            people: '👋',
            nature: '🌿',
            food: '🍕',
            objects: '💡',
            symbols: '❤️',
          }[id]
        }}
      </button>
    </div>
    <div ref="grid" class="picker__grid" @scroll.passive="onScroll" @keydown="onGridKey">
      <p v-if="loadState === 'loading'" class="picker__note" aria-busy="true">
        {{ t('discuss.picker.loading') }}
      </p>
      <p v-else-if="loadState === 'failed'" class="picker__note" role="alert">
        {{ t('discuss.picker.loadFailed') }}
      </p>
      <template v-else-if="searching">
        <span class="picker__label">{{ t('discuss.picker.results') }}</span>
        <p v-if="results.length === 0" class="picker__note">
          {{ t('discuss.picker.noResults', { query: query.trim() }) }}
        </p>
        <div
          v-else
          role="group"
          class="picker__emoji-grid"
          :aria-label="t('discuss.picker.results')"
        >
          <button
            v-for="e in results"
            :key="e.emoji"
            type="button"
            class="picker__emoji"
            :class="{ 'picker__emoji--mine': isMine(e.emoji) }"
            :aria-label="t('discuss.picker.emoji', { emoji: e.name })"
            :data-emoji="e.emoji"
            @mouseenter="previewed = e.emoji"
            @focus="previewed = e.emoji"
            @click="pick(e.emoji)"
          >
            {{ e.emoji }}
          </button>
        </div>
      </template>
      <template v-else>
        <template v-for="section in sections" :key="section.id">
          <span class="picker__label" :data-section="section.id">
            {{ t(`discuss.picker.${section.id}`) }}
          </span>
          <p v-if="section.emojis.length === 0" class="picker__note">
            {{ t('discuss.picker.noRecent') }}
          </p>
          <div
            v-else
            role="group"
            class="picker__emoji-grid"
            :aria-label="t(`discuss.picker.${section.id}`)"
          >
            <button
              v-for="e in section.emojis"
              :key="e.emoji"
              type="button"
              class="picker__emoji"
              :class="{ 'picker__emoji--mine': isMine(e.emoji) }"
              :aria-label="t('discuss.picker.emoji', { emoji: e.name })"
              :data-emoji="e.emoji"
              @mouseenter="previewed = e.emoji"
              @focus="previewed = e.emoji"
              @click="pick(e.emoji)"
            >
              {{ e.emoji }}
            </button>
          </div>
        </template>
      </template>
    </div>
    <div class="picker__preview" aria-hidden="true">
      <span class="picker__preview-emoji">{{ preview.emoji }}</span>
      <span class="picker__preview-text">
        <span class="picker__preview-name">{{ preview.name }}</span>
        <span v-if="preview.slug" class="picker__preview-hint">
          {{ t('discuss.picker.hint', { slug: preview.slug }) }}
        </span>
      </span>
      <span class="picker__limit">{{ t('discuss.picker.limit') }}</span>
    </div>
  </div>
</template>

<style scoped>
.picker {
  position: absolute;
  top: calc(100% + 8px);
  left: -8px;
  z-index: 10;
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 340px;
  max-width: calc(100vw - 32px);
  box-sizing: border-box;
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  background: var(--surface);
  box-shadow: var(--shadow-menu);
  /* Scrolled into view clear of the sticky meeting footer. */
  scroll-margin: 16px 16px 96px;
}
.picker__quick {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.picker__label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.picker__quick-row {
  display: flex;
  justify-content: space-between;
}
.picker__quick-emoji,
.picker__emoji {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  line-height: 1;
  cursor: pointer;
}
.picker__quick-emoji {
  width: 36px;
  height: 36px;
  font-size: 21px;
}
.picker__quick-emoji[aria-pressed='true'],
.picker__emoji--mine {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.picker__quick-emoji:hover,
.picker__emoji:hover {
  background: var(--well);
}
.picker__search {
  position: relative;
}
.picker__search-icon {
  position: absolute;
  top: 12px;
  left: 12px;
  display: flex;
  color: var(--muted);
}
.picker__input {
  width: 100%;
  height: 40px;
  box-sizing: border-box;
  padding: 0 12px 0 36px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--well);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 14px;
}
.picker__tabs {
  display: flex;
  border-bottom: 1px solid var(--line);
}
.picker__tab {
  flex: 1 1 0;
  height: 34px;
  padding: 0;
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  font-size: 17px;
  line-height: 1;
  opacity: 0.6;
  cursor: pointer;
}
.picker__tab[aria-selected='true'] {
  border-bottom-color: var(--accent);
  opacity: 1;
}
.picker__grid {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  /* Fixed: the picker keeps its size while the emoji load. */
  height: 176px;
  padding-right: 2px;
  overflow: auto;
}
.picker__emoji-grid {
  display: grid;
  grid-template-columns: repeat(8, minmax(0, 1fr));
  gap: 2px;
}
.picker__emoji {
  width: 34px;
  height: 34px;
  font-size: 20px;
}
.picker__note {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
}
.picker__preview {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}
.picker__preview-emoji {
  font-size: 26px;
  line-height: 1;
}
.picker__preview-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.picker__preview-name {
  font-size: 14px;
  font-weight: 600;
}
.picker__preview-hint,
.picker__limit {
  color: var(--muted);
  font-size: 12px;
}
.picker__limit {
  margin-left: auto;
  text-align: right;
}
</style>
