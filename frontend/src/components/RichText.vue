<script setup lang="ts">
/**
 * Read-only rich text (ADR-0010). The HTML is re-sanitised here against the
 * stored subset (same tags, attributes and link schemes the server keeps), so
 * nothing outside it is rendered. Images carry an attachment id and no `src`
 * (ADR-0013): each is fetched with the access token and shown as a `blob:` URL.
 */
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { attachmentUrl } from '@/lib/attachments';
import { loadImage } from '@/lib/images';
import { sanitizeRichText } from '@/lib/sanitize';

const props = defineProps<{ html: string }>();
const { t } = useI18n();

const root = ref<HTMLElement | null>(null);
const safeHtml = computed(() => sanitizeRichText(props.html));

/** Gives every image of the rendered HTML its authenticated source. */
function loadImages(): void {
  const images = root.value?.querySelectorAll<HTMLImageElement>('img[data-attachment-id]') ?? [];
  for (const img of images) {
    const id = img.dataset.attachmentId ?? '';
    img.decoding = 'async';
    if (!img.alt) img.alt = t('richText.image.alt');
    loadImage(attachmentUrl(id)).then(
      (url) => {
        // The HTML may have been re-rendered meanwhile.
        if (root.value?.contains(img)) img.src = url;
      },
      () => img.classList.add('rich-text__broken'),
    );
  }
}

onMounted(loadImages);
watch(safeHtml, () => nextTick(loadImages));
</script>

<template>
  <!-- eslint-disable vue/no-v-html -- sanitised by DOMPurify in sanitizeRichText (ADR-0010) -->
  <!-- nosemgrep: javascript.vue.security.audit.xss.templates.avoid-v-html.avoid-v-html -->
  <div ref="root" class="rich-text" v-html="safeHtml"></div>
  <!-- eslint-enable vue/no-v-html -->
</template>

<style scoped>
.rich-text {
  overflow-wrap: anywhere;
}

.rich-text :deep(p) {
  margin: 0;
}

.rich-text :deep(p + p) {
  margin-top: 8px;
}

.rich-text :deep(a) {
  color: var(--accent-text);
}

.rich-text :deep(a:hover) {
  color: var(--text);
}

.rich-text :deep(span[data-type='mention']) {
  background: var(--accent-soft);
  color: var(--accent-text);
  border-radius: var(--radius-xs);
  padding: 0 6px;
  font-weight: 600;
}
.rich-text :deep(ul) {
  margin: 0;
  padding-left: 20px;
}

.rich-text :deep(img) {
  display: block;
  max-width: 100%;
  height: auto;
  max-height: 240px;
  margin: 8px 0;
  border-radius: var(--radius-sm);
  object-fit: contain;
  object-position: left;
}

/* An image that could not be loaded keeps its space, framed. */
.rich-text :deep(img.rich-text__broken) {
  min-width: 44px;
  min-height: 44px;
  border: 1px dashed var(--line);
  background: var(--well);
}

.rich-text :deep(code) {
  padding: 1px 4px;
  border-radius: var(--radius-xs);
  background: var(--well);
  font-size: 0.93em;
}
</style>
