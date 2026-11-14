<script setup lang="ts">
/**
 * Rich text editor for the subset ADR-0010 stores: paragraphs, line breaks,
 * bold, italic, http(s)/mailto links and `@` mentions. Everything else is
 * switched off so the editor cannot produce content the server would strip.
 */
import Mention from '@tiptap/extension-mention';
import StarterKit from '@tiptap/starter-kit';
import { EditorContent, useEditor } from '@tiptap/vue-3';
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import type { PersonDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';

const props = defineProps<{ modelValue: string; members: PersonDto[]; label: string }>();
const emit = defineEmits<{ 'update:modelValue': [html: string] }>();
const { t } = useI18n();

const MAX_SUGGESTIONS = 6;
/** The only link schemes the server keeps (ADR-0010); Tiptap's own list is broader. */
const ALLOWED_LINK = /^(?:https?|mailto):/i;
const isAllowedLink = (url: string): boolean => ALLOWED_LINK.test(url.trim());
const uid = useId();
const listId = `mentions-${uid}`;
const urlId = `link-url-${uid}`;
const optionId = (i: number) => `${listId}-${i}`;

interface SuggestionState {
  items: PersonDto[];
  command: (attrs: { id: string; label: string }) => void;
}

/** Open mention suggestions, or `null` while the `@` menu is closed. */
const suggestion = ref<SuggestionState | null>(null);
const selectedIndex = ref(0);

function selectItem(index: number): void {
  const state = suggestion.value;
  const person = state?.items[index];
  if (state && person) state.command({ id: person.id, label: person.displayName });
}

/** The server strips `data-mention-suggestion-char`; keep it out of the markup. */
const RetroMention = Mention.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      mentionSuggestionChar: { default: '@', rendered: false },
    };
  },
});

const editor = useEditor({
  content: props.modelValue,
  editorProps: {
    attributes: {
      role: 'textbox',
      'aria-multiline': 'true',
      'aria-autocomplete': 'list',
      'aria-label': props.label,
      class: 'rte-content',
    },
  },
  extensions: [
    StarterKit.configure({
      heading: false,
      bulletList: false,
      orderedList: false,
      listItem: false,
      listKeymap: false,
      blockquote: false,
      code: false,
      codeBlock: false,
      horizontalRule: false,
      strike: false,
      underline: false,
      link: {
        openOnClick: false,
        autolink: true,
        protocols: ['http', 'https', 'mailto'],
        isAllowedUri: (url) => isAllowedLink(url),
        shouldAutoLink: (url) => isAllowedLink(url),
        HTMLAttributes: { rel: 'noopener noreferrer', target: null, class: null },
      },
    }),
    RetroMention.configure({
      HTMLAttributes: { 'data-type': 'mention' },
      suggestion: {
        char: '@',
        items: ({ query }) => {
          const needle = query.trim().toLowerCase();
          return props.members
            .filter((m) => m.displayName.toLowerCase().includes(needle))
            .slice(0, MAX_SUGGESTIONS);
        },
        render: () => ({
          onStart: (p) => {
            selectedIndex.value = 0;
            suggestion.value = { items: p.items, command: p.command };
          },
          onUpdate: (p) => {
            selectedIndex.value = 0;
            suggestion.value = { items: p.items, command: p.command };
          },
          onExit: () => {
            suggestion.value = null;
          },
          onKeyDown: ({ event }) => {
            const count = suggestion.value?.items.length ?? 0;
            if (count === 0) return false;
            if (event.key === 'Escape') {
              // The plugin closes the menu; keep the Esc from also closing a parent dialog.
              event.stopPropagation();
              event.preventDefault();
              return true;
            }
            if (event.key === 'ArrowDown') {
              selectedIndex.value = (selectedIndex.value + 1) % count;
              return true;
            }
            if (event.key === 'ArrowUp') {
              selectedIndex.value = (selectedIndex.value - 1 + count) % count;
              return true;
            }
            if (event.key === 'Enter') {
              selectItem(selectedIndex.value);
              return true;
            }
            return false;
          },
        }),
      },
    }),
  ],
  onUpdate: ({ editor: e }) => emit('update:modelValue', e.getHTML()),
});

watch(
  () => props.modelValue,
  (value) => {
    const e = editor.value;
    if (e && value !== e.getHTML()) e.commands.setContent(value, { emitUpdate: false });
  },
);

watch(
  () => props.label,
  (label) => editor.value?.view.dom.setAttribute('aria-label', label),
);

/** Point the textbox at the highlighted option while the menu is open. */
watch([suggestion, selectedIndex], () => {
  const dom = editor.value?.view.dom;
  if (!dom) return;
  if (suggestion.value && suggestion.value.items.length > 0) {
    dom.setAttribute('aria-expanded', 'true');
    dom.setAttribute('aria-controls', listId);
    dom.setAttribute('aria-activedescendant', optionId(selectedIndex.value));
  } else {
    dom.removeAttribute('aria-expanded');
    dom.removeAttribute('aria-controls');
    dom.removeAttribute('aria-activedescendant');
  }
});

// --- Link field -------------------------------------------------------------

const linkOpen = ref(false);
const linkError = ref(false);
const errorId = `link-error-${uid}`;
const linkUrl = ref('');
const linkInput = ref<HTMLInputElement | null>(null);
const linkButton = ref<HTMLButtonElement | null>(null);

const linkActive = computed(() => editor.value?.isActive('link') ?? false);

async function toggleLink(): Promise<void> {
  if (linkOpen.value) {
    closeLink(true);
    return;
  }
  const attrs = editor.value?.getAttributes('link') as { href?: string } | undefined;
  linkUrl.value = attrs?.href ?? '';
  linkError.value = false;
  linkOpen.value = true;
  await nextTick();
  linkInput.value?.focus();
  linkInput.value?.select();
}

function closeLink(refocus: boolean): void {
  linkOpen.value = false;
  if (refocus) editor.value?.commands.focus();
}

/** Bare domains get https://; schemes are validated by `isAllowedLink` afterwards. */
function normaliseUrl(raw: string): string {
  const value = raw.trim();
  if (value === '' || /^[a-z][a-z0-9+.-]*:/i.test(value)) return value;
  return `https://${value}`;
}

function applyLink(): void {
  const e = editor.value;
  if (!e) return;
  const href = normaliseUrl(linkUrl.value);
  if (href !== '' && !isAllowedLink(href)) {
    linkError.value = true;
    linkInput.value?.focus();
    return;
  }
  const chain = e.chain().focus();
  if (href === '') {
    chain.extendMarkRange('link').unsetLink().run();
  } else if (e.state.selection.empty && !e.isActive('link')) {
    // Nothing selected: insert the address itself as the link text.
    chain
      .insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] })
      .run();
  } else {
    chain.extendMarkRange('link').setLink({ href }).run();
  }
  linkOpen.value = false;
}

onBeforeUnmount(() => {
  suggestion.value = null;
});
</script>

<template>
  <div class="rte">
    <div class="rte-toolbar" role="toolbar" :aria-label="t('richText.toolbar')">
      <button
        type="button"
        class="rte-btn"
        :aria-label="t('richText.bold')"
        :aria-pressed="editor?.isActive('bold') ?? false"
        @click="editor?.chain().focus().toggleBold().run()"
      >
        <AppIcon name="bold" :size="16" />
      </button>
      <button
        type="button"
        class="rte-btn"
        :aria-label="t('richText.italic')"
        :aria-pressed="editor?.isActive('italic') ?? false"
        @click="editor?.chain().focus().toggleItalic().run()"
      >
        <AppIcon name="italic" :size="16" />
      </button>
      <button
        ref="linkButton"
        type="button"
        class="rte-btn"
        :aria-label="t('richText.link')"
        :aria-pressed="linkActive"
        :aria-expanded="linkOpen"
        @click="toggleLink"
      >
        <AppIcon name="link" :size="16" />
      </button>
    </div>

    <form
      v-if="linkOpen"
      class="rte-link"
      @submit.prevent="applyLink"
      @keydown.esc.stop.prevent="closeLink(true)"
    >
      <label class="rte-link-label" :for="urlId">{{ t('richText.linkUrl') }}</label>
      <input
        :id="urlId"
        ref="linkInput"
        v-model="linkUrl"
        :aria-invalid="linkError"
        :aria-describedby="linkError ? errorId : undefined"
        @input="linkError = false"
        class="rte-link-input"
        type="text"
        inputmode="url"
        autocomplete="off"
        placeholder="https://"
      />
      <p v-if="linkError" :id="errorId" class="rte-link-error" role="alert">
        {{ t('richText.linkInvalid') }}
      </p>
      <button type="submit" class="rte-link-btn rte-link-apply">
        {{ t('richText.linkApply') }}
      </button>
      <button type="button" class="rte-link-btn" @click="closeLink(true)">
        {{ t('richText.linkCancel') }}
      </button>
    </form>

    <div class="rte-area">
      <EditorContent :editor="editor" />
      <ul
        v-if="suggestion && suggestion.items.length > 0"
        :id="listId"
        class="rte-menu"
        role="listbox"
        :aria-label="t('richText.mentions')"
        @mousedown.prevent
      >
        <li
          v-for="(person, i) in suggestion.items"
          :id="optionId(i)"
          :key="person.id"
          class="rte-option"
          role="option"
          :aria-selected="i === selectedIndex"
          @click="selectItem(i)"
          @mousemove="selectedIndex = i"
        >
          {{ person.displayName }}
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.rte {
  position: relative;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
}

.rte:focus-within {
  border-color: var(--accent);
  outline: 2px solid var(--accent);
  outline-offset: 0;
}

.rte-toolbar {
  display: flex;
  gap: 4px;
  padding: 4px;
  border-bottom: 1px solid var(--line);
}

.rte-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}

.rte-btn:hover {
  color: var(--text);
}

.rte-btn[aria-pressed='true'] {
  background: var(--accent-soft);
  color: var(--accent-text);
}

.rte-link {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border-bottom: 1px solid var(--line);
}

.rte-link-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
}

.rte-link-input {
  flex: 1 1 180px;
  min-width: 0;
  min-height: 44px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--surface);
  color: var(--text);
  font: inherit;
}

.rte-link-error {
  flex: 1 0 100%;
  margin: 0;
  font-size: 13px;
  color: var(--danger);
}

.rte-link-btn {
  min-height: 44px;
  padding: 0 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}

.rte-link-apply {
  border-color: var(--accent);
  background: var(--accent);
  color: var(--on-accent);
}

.rte-area {
  position: relative;
}

.rte-area :deep(.rte-content) {
  min-height: 96px;
  padding: 10px 12px;
  outline: none;
  overflow-wrap: anywhere;
}

.rte-area :deep(.rte-content p) {
  margin: 0;
}

.rte-area :deep(.rte-content p + p) {
  margin-top: 8px;
}

.rte-area :deep(.rte-content a) {
  color: var(--accent-text);
}

.rte-area :deep(.rte-content a:hover) {
  color: var(--text);
}

.rte-area :deep(span[data-type='mention']) {
  background: var(--accent-soft);
  color: var(--accent-text);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  font-weight: 600;
}

.rte-menu {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: 10;
  min-width: 220px;
  max-width: 100%;
  margin: 4px 0 0;
  padding: 4px;
  list-style: none;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  box-shadow: var(--shadow-menu);
}

.rte-option {
  display: flex;
  align-items: center;
  min-height: 44px;
  padding: 0 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.rte-option[aria-selected='true'] {
  background: var(--accent-soft);
  color: var(--accent-text);
}
</style>
