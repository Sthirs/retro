<script setup lang="ts">
/**
 * Read-only rich text (ADR-0010). The HTML is re-sanitised here against the
 * stored subset (same tags, attributes and link schemes the server keeps), so
 * nothing outside it is rendered.
 */
import { computed } from 'vue';

import { sanitizeRichText } from '@/lib/sanitize';

const props = defineProps<{ html: string }>();

const safeHtml = computed(() => sanitizeRichText(props.html));
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html -- sanitised by DOMPurify in sanitizeRichText -->
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
