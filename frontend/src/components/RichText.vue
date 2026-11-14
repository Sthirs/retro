<script setup lang="ts">
/**
 * Read-only rich text (ADR-0010). The HTML is re-sanitised here with the same
 * allowlist the server applies, so nothing but the stored subset is rendered.
 */
import DOMPurify from 'dompurify';
import { computed } from 'vue';

const props = defineProps<{ html: string }>();

const safeHtml = computed(() =>
  DOMPurify.sanitize(props.html, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 'a', 'span'],
    ALLOWED_ATTR: ['href', 'rel', 'data-type', 'data-id', 'data-label'],
  }),
);
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html -- sanitised by DOMPurify above -->
  <div class="rich-text" v-html="safeHtml"></div>
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
</style>
