<script setup lang="ts">
/**
 * Rich text editor for the subset ADR-0010 stores: paragraphs, line breaks,
 * bold, italic and http(s)/mailto links, plus per `kind`:
 * - `task`: `@` mentions;
 * - `reflection`: strikethrough, bullet lists and inline code (spec §9.2), in
 *   the compact Reflect composer (toolbar under the text, a placeholder,
 *   Ctrl/Cmd + Enter submits, Esc cancels).
 * With a `teamId`, images can be pasted, dropped or picked (ADR-0013): they
 * are uploaded at once and stored as `<img data-attachment-id>`, never with a
 * `src`. Everything else is switched off so the editor cannot produce content
 * the server would strip.
 */
import Mention from '@tiptap/extension-mention';
import StarterKit from '@tiptap/starter-kit';
import {
  EditorContent,
  Extension,
  Node,
  useEditor,
  type AnyExtension,
  type Editor,
} from '@tiptap/vue-3';
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { errorCode, type PersonDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import { ACCEPTED_TYPES, attachmentUrl, uploadAttachment } from '@/lib/attachments';
import { loadImage } from '@/lib/images';
import { ATTACHMENT_ID } from '@/lib/sanitize';
import '@/styles/prosemirror.css';

const props = withDefaults(
  defineProps<{
    modelValue: string;
    members?: PersonDto[];
    label: string;
    kind?: 'task' | 'reflection';
    placeholder?: string;
    autofocus?: boolean;
    /** Locks the text, e.g. while it is being sent. */
    readonly?: boolean;
    /** The team images are uploaded to; without it, images are off. */
    teamId?: string;
  }>(),
  {
    members: () => [],
    kind: 'task',
    placeholder: '',
    autofocus: false,
    readonly: false,
    teamId: undefined,
  },
);
const emit = defineEmits<{
  'update:modelValue': [html: string];
  submit: [];
  cancel: [];
  /** True while images are uploading: the content is not final yet. */
  uploading: [busy: boolean];
}>();
const reflection = props.kind === 'reflection';
const { t } = useI18n();

const MAX_SUGGESTIONS = 6;
/** The only link schemes the server keeps (ADR-0010); Tiptap's own list is broader. */
const ALLOWED_LINK = /^(?:https?|mailto):/i;
/** Whitespace and control characters that browsers (and Tiptap) drop from a URL before parsing it. */
// eslint-disable-next-line no-control-regex -- stripping control characters is the point
const URL_NOISE = /[\u0000-\u0020\u007f-\u00a0\u1680\u180e\u2000-\u2029\u202f\u205f\u3000\ufeff]/g;
const cleanUrl = (url: string): string => url.replace(URL_NOISE, '');
/** A scheme-like prefix, or any `:` before the first `/`, `?` or `#`: never treat that as scheme-less. */
const hasScheme = (cleaned: string): boolean =>
  /^[a-z][a-z0-9+.-]*:/i.test(cleaned) || /^[^/?#]*:/.test(cleaned);
const isAllowedScheme = (url: string): boolean => ALLOWED_LINK.test(cleanUrl(url));
/**
 * Tiptap hands autolink the text it matched (`example.com`, `a@b.c`), not the
 * href, and completes the scheme itself (http, or mailto for emails). So a
 * scheme-less value is fine; any explicit scheme must be an allowed one.
 */
const isAllowedLink = (url: string): boolean => !hasScheme(cleanUrl(url)) || isAllowedScheme(url);

/** Tiptap's own guard: no autolink for bare IPv4 addresses or dotless hostnames. */
function defaultShouldAutoLink(url: string): boolean {
  if (hasScheme(url) && !url.includes('@')) return true;
  const hostname = (url.includes('@') ? url.split('@').pop() : url)?.split(/[/?#:]/)[0] ?? '';
  return !/^\d{1,3}(\.\d{1,3}){3}$/.test(hostname) && hostname.includes('.');
}

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

/** Most images in one content, as the server allows (spec §13). */
const MAX_IMAGES = 10;

/**
 * An uploaded image. It parses only `img[data-attachment-id]`, so images
 * pasted from other sites (`<img src>`) are dropped, and it renders no `src`:
 * the node view loads the image with the access token.
 */
const AttachmentImage = Node.create({
  name: 'attachmentImage',
  group: 'inline',
  inline: true,
  atom: true,
  draggable: true,
  selectable: true,
  addAttributes() {
    const numeric = (name: string) => ({
      default: null,
      parseHTML: (el: HTMLElement) => {
        const value = el.getAttribute(name);
        return value && /^[1-9]\d{0,4}$/.test(value) ? Number(value) : null;
      },
      renderHTML: (attrs: Record<string, unknown>) =>
        attrs[name] ? { [name]: String(attrs[name]) } : {},
    });
    return {
      attachmentId: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute('data-attachment-id'),
        renderHTML: (attrs: Record<string, unknown>) => ({
          'data-attachment-id': attrs.attachmentId,
        }),
      },
      alt: {
        default: '',
        parseHTML: (el: HTMLElement) => el.getAttribute('alt') ?? '',
        renderHTML: (attrs: Record<string, unknown>) => ({ alt: attrs.alt }),
      },
      width: numeric('width'),
      height: numeric('height'),
    };
  },
  parseHTML() {
    return [
      {
        tag: 'img[data-attachment-id]',
        getAttrs: (el: HTMLElement) =>
          ATTACHMENT_ID.test(el.getAttribute('data-attachment-id') ?? '') ? null : false,
      },
    ];
  },
  renderHTML({ HTMLAttributes }) {
    return ['img', HTMLAttributes];
  },
  addNodeView() {
    return ({ node }) => {
      const dom = document.createElement('img');
      const id = String(node.attrs.attachmentId);
      dom.className = 'rte-image';
      dom.dataset.attachmentId = id;
      dom.alt = String(node.attrs.alt || t('richText.image.alt'));
      if (node.attrs.width) dom.width = Number(node.attrs.width);
      if (node.attrs.height) dom.height = Number(node.attrs.height);
      loadImage(attachmentUrl(id)).then(
        (url) => {
          dom.src = url;
        },
        () => dom.classList.add('rte-image--broken'),
      );
      return { dom };
    };
  },
});

/** Reflections: Ctrl/Cmd + Enter sends, Esc cancels. */
const Shortcuts = Extension.create({
  name: 'retroShortcuts',
  addKeyboardShortcuts() {
    return {
      'Mod-Enter': () => {
        emit('submit');
        return true;
      },
      Escape: () => {
        if (linkOpen.value) return false;
        emit('cancel');
        return true;
      },
    };
  },
});

const editor = useEditor({
  content: props.modelValue,
  editable: !props.readonly,
  autofocus: props.autofocus ? 'end' : false,
  // The CSP forbids inline <style>: the base styles come from prosemirror.css.
  injectCSS: false,
  editorProps: {
    attributes: {
      role: 'textbox',
      'aria-multiline': 'true',
      'aria-autocomplete': 'list',
      'aria-label': props.label,
      class: 'rte-content',
    },
    handlePaste: (_view, event) => takeFiles(event.clipboardData?.files, event),
    handleDrop: (view, event, _slice, moved) => {
      if (moved) return false;
      const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
      return takeFiles(event.dataTransfer?.files, event, at);
    },
  },
  extensions: [
    StarterKit.configure({
      heading: false,
      bulletList: reflection ? {} : false,
      orderedList: false,
      listItem: reflection ? {} : false,
      listKeymap: reflection ? {} : false,
      blockquote: false,
      code: reflection ? {} : false,
      codeBlock: false,
      horizontalRule: false,
      strike: reflection ? {} : false,
      underline: false,
      link: {
        openOnClick: false,
        autolink: true,
        protocols: ['http', 'https', 'mailto'],
        isAllowedUri: (url) => isAllowedLink(url),
        shouldAutoLink: (url) => isAllowedLink(url) && defaultShouldAutoLink(url),
        HTMLAttributes: { rel: 'noopener noreferrer', target: null, class: null },
      },
    }),
    ...(reflection ? [Shortcuts] : [mentions()]),
    ...(props.teamId ? [AttachmentImage] : []),
  ],
  onUpdate: ({ editor: e }) => emit('update:modelValue', e.getHTML()),
});

/** Task mentions, with their own ARIA listbox. */
function mentions(): AnyExtension {
  return RetroMention.configure({
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
  });
}

const empty = computed(() => editor.value?.isEmpty ?? props.modelValue === '');

defineExpose({ focus: () => editor.value?.commands.focus('end') });

watch(
  () => props.modelValue,
  (value) => {
    const e = editor.value;
    if (e && value !== e.getHTML()) e.commands.setContent(value, { emitUpdate: false });
  },
);

/** The focus when the text was locked, to give it back on unlock. */
let focusedWhenLocked = false;
watch(
  () => props.readonly,
  (readonly) => {
    const e = editor.value;
    if (!e) return;
    if (readonly) focusedWhenLocked = e.isFocused;
    e.setEditable(!readonly, false);
    e.view.dom.setAttribute('aria-readonly', String(readonly));
    if (!readonly && focusedWhenLocked) e.commands.focus('end');
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

// --- Images -------------------------------------------------------------------

const fileInput = ref<HTMLInputElement | null>(null);
const imageErrorId = `image-error-${uid}`;
/** Uploads in flight. */
const uploads = ref(0);
/** The last upload's error, as a `richText.image.errors` key. */
const imageError = ref<string | null>(null);
const IMAGE_ERRORS = [
  'image_too_large',
  'unsupported_image_type',
  'invalid_image',
  'image_dimensions_too_large',
  'too_many_requests',
  'too_many_pending_attachments',
  'too_many_images',
];

watch(
  () => uploads.value > 0,
  (busy) => emit('uploading', busy),
);

function imageCount(e: Editor): number {
  let count = 0;
  e.state.doc.descendants((node) => {
    if (node.type.name === 'attachmentImage') count += 1;
  });
  return count;
}

/**
 * Uploads the image files of a paste or drop, if any, and inserts them at
 * `at` (default: the selection). Returns whether the event was handled.
 */
function takeFiles(list: FileList | undefined, event: Event, at?: number): boolean {
  const files = Array.from(list ?? []).filter((f) => f.type.startsWith('image/'));
  if (!props.teamId || props.readonly || files.length === 0) return false;
  event.preventDefault();
  void insertImages(files, at);
  return true;
}

async function insertImages(files: File[], at?: number): Promise<void> {
  const e = editor.value;
  const teamId = props.teamId;
  if (!e || !teamId) return;
  imageError.value = null;
  if (imageCount(e) + uploads.value + files.length > MAX_IMAGES) {
    imageError.value = 'too_many_images';
    return;
  }
  let position = at;
  for (const file of files) {
    uploads.value += 1;
    try {
      const image = await uploadAttachment(teamId, file);
      if (e.isDestroyed) return;
      const node = {
        type: 'attachmentImage',
        attrs: { attachmentId: image.id, alt: '', width: image.width, height: image.height },
      };
      const chain = e.chain().focus();
      if (position !== undefined && position <= e.state.doc.content.size) {
        chain.insertContentAt(position, node).run();
        position += 1;
      } else {
        chain.insertContent(node).run();
      }
    } catch (err) {
      const code = errorCode(err);
      imageError.value = IMAGE_ERRORS.includes(code) ? code : 'unknown';
    } finally {
      uploads.value -= 1;
    }
  }
}

function chooseImages(): void {
  fileInput.value?.click();
}

function onFiles(event: Event): void {
  const input = event.target as HTMLInputElement;
  const files = Array.from(input.files ?? []);
  input.value = '';
  if (files.length > 0) void insertImages(files);
}

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
  const value = cleanUrl(raw);
  if (value === '' || hasScheme(value)) return value;
  return `https://${value}`;
}

function applyLink(): void {
  const e = editor.value;
  if (!e) return;
  const href = normaliseUrl(linkUrl.value);
  if (href !== '' && !isAllowedScheme(href)) {
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
  <div class="rte" :class="{ 'rte--reflection': reflection }">
    <!-- Pointer presses on the toolbar keep the editor's focus and selection. -->
    <div class="rte-toolbar" role="toolbar" :aria-label="t('richText.toolbar')">
      <button
        type="button"
        class="rte-btn"
        :aria-label="t('richText.bold')"
        :aria-pressed="editor?.isActive('bold') ?? false"
        @mousedown.prevent
        @click="editor?.chain().focus().toggleBold().run()"
      >
        <AppIcon name="bold" :size="16" />
      </button>
      <button
        type="button"
        class="rte-btn"
        :aria-label="t('richText.italic')"
        :aria-pressed="editor?.isActive('italic') ?? false"
        @mousedown.prevent
        @click="editor?.chain().focus().toggleItalic().run()"
      >
        <AppIcon name="italic" :size="16" />
      </button>
      <button
        v-if="reflection"
        type="button"
        class="rte-btn"
        :aria-label="t('richText.strike')"
        :aria-pressed="editor?.isActive('strike') ?? false"
        @mousedown.prevent
        @click="editor?.chain().focus().toggleStrike().run()"
      >
        <AppIcon name="strikethrough" :size="16" />
      </button>
      <button
        ref="linkButton"
        type="button"
        class="rte-btn"
        :aria-label="t('richText.link')"
        :aria-pressed="linkActive"
        :aria-expanded="linkOpen"
        @mousedown.prevent
        @click="toggleLink"
      >
        <AppIcon name="link" :size="16" />
      </button>
      <template v-if="reflection">
        <button
          type="button"
          class="rte-btn"
          :aria-label="t('richText.bulletList')"
          :aria-pressed="editor?.isActive('bulletList') ?? false"
          @mousedown.prevent
          @click="editor?.chain().focus().toggleBulletList().run()"
        >
          <AppIcon name="list" :size="16" />
        </button>
        <button
          type="button"
          class="rte-btn"
          :aria-label="t('richText.code')"
          :aria-pressed="editor?.isActive('code') ?? false"
          @mousedown.prevent
          @click="editor?.chain().focus().toggleCode().run()"
        >
          <AppIcon name="code" :size="16" />
        </button>
      </template>
      <template v-if="teamId">
        <button
          type="button"
          class="rte-btn"
          :aria-label="t('richText.image.add')"
          :aria-describedby="imageError ? imageErrorId : undefined"
          :disabled="readonly"
          @mousedown.prevent
          @click="chooseImages"
        >
          <AppIcon name="image" :size="16" />
        </button>
        <input
          ref="fileInput"
          type="file"
          class="rte-file"
          :accept="ACCEPTED_TYPES.join(',')"
          multiple
          tabindex="-1"
          aria-hidden="true"
          @change="onFiles"
        />
        <span v-if="uploads > 0" class="rte-status" role="status">
          {{ t('richText.image.uploading') }}
        </span>
      </template>
    </div>
    <p v-if="imageError" :id="imageErrorId" class="rte-image-error" role="alert">
      {{ t(`richText.image.errors.${imageError}`) }}
    </p>

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
        class="rte-link-input"
        type="text"
        inputmode="url"
        autocomplete="off"
        placeholder="https://"
        @input="linkError = false"
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
      <span v-if="placeholder && empty" class="rte-placeholder" aria-hidden="true">
        {{ placeholder }}
      </span>
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

.rte-file {
  display: none;
}

.rte-status {
  align-self: center;
  padding: 0 8px;
  font-size: 13px;
  color: var(--muted);
}

.rte-image-error {
  margin: 0;
  padding: 8px 12px;
  font-size: 13px;
  color: var(--danger);
}

.rte-area :deep(.rte-image) {
  display: inline-block;
  max-width: 100%;
  height: auto;
  max-height: 200px;
  margin: 4px 0;
  vertical-align: bottom;
  border-radius: var(--radius-sm);
  object-fit: contain;
}

.rte-area :deep(.rte-image.ProseMirror-selectednode) {
  outline: 2px solid var(--accent);
}

.rte-area :deep(.rte-image--broken) {
  min-width: 44px;
  min-height: 44px;
  border: 1px dashed var(--line);
  background: var(--well);
}

.rte-placeholder {
  position: absolute;
  top: 10px;
  left: 12px;
  color: var(--muted);
  pointer-events: none;
}

.rte-area :deep(.rte-content ul) {
  margin: 0;
  padding-left: 20px;
}

.rte-area :deep(.rte-content code) {
  padding: 1px 4px;
  border-radius: var(--radius-xs);
  background: var(--well);
  font-size: 13px;
}

/* Reflect composer (Reflect artboard): text first, then a compact toolbar;
   the composer around it draws the frame. */
.rte--reflection {
  display: flex;
  flex-direction: column;
  border: 0;
  border-radius: 0;
  background: transparent;
}

.rte--reflection:focus-within {
  outline: none;
}

.rte--reflection .rte-area {
  order: -1;
}

.rte--reflection .rte-area :deep(.rte-content) {
  min-height: 64px;
  padding: 12px 14px;
  font-size: 14px;
  line-height: 1.5;
}

.rte--reflection .rte-placeholder {
  top: 12px;
  left: 14px;
  font-size: 14px;
  line-height: 1.5;
}

.rte--reflection .rte-toolbar {
  flex-wrap: wrap;
  gap: 0;
  padding: 2px 4px;
  border-top: 1px solid var(--line);
  border-bottom: 0;
}

.rte--reflection .rte-btn {
  width: 36px;
  height: 36px;
  position: relative;
  border-radius: var(--radius-control);
}
.rte--reflection .rte-btn::before {
  /* 44 px touch target around the 36 px control. */
  content: '';
  position: absolute;
  inset: -4px;
}

.rte--reflection .rte-link {
  border-top: 1px solid var(--line);
  border-bottom: 0;
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
