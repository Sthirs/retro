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
  <!-- eslint-disable vue/no-v-html -- sanitised by DOMPurify in sanitizeRichText (ADR-0010) -->
  <!-- nosemgrep: javascript.vue.security.audit.xss.templates.avoid-v-html.avoid-v-html -->
  <div class="rich-text" v-html="safeHtml"></div>
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

.rich-text :deep(code) {
  padding: 1px 4px;
  border-radius: var(--radius-xs);
  background: var(--well);
  font-size: 0.93em;
}
</style>
