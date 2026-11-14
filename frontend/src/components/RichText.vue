<script setup lang="ts">
/**
 * Read-only rich text (ADR-0010). The HTML is re-sanitised here against the
 * stored subset (same tags, attributes and link schemes the server keeps), so
 * nothing outside it is rendered.
 */
import DOMPurify from 'dompurify';
import { computed } from 'vue';

// The server always sets rel on links; do the same so stored HTML can't opt out.
DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A' && node.hasAttribute('href')) {
    node.setAttribute('rel', 'noopener noreferrer');
  }
});

const props = defineProps<{ html: string }>();

const safeHtml = computed(() =>
  DOMPurify.sanitize(props.html, {
    ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 'a', 'span'],
    ALLOWED_ATTR: ['href', 'rel', 'data-type', 'data-id', 'data-label'],
    ALLOW_DATA_ATTR: false,
    // Plain-text values, not URIs: exempt them from the scheme check below.
    ADD_URI_SAFE_ATTR: ['rel', 'data-type', 'data-id', 'data-label'],
    ALLOWED_URI_REGEXP: /^(?:https?|mailto):/i,
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
