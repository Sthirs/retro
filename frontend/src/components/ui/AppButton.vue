<script setup lang="ts">
/**
 * Button (or link styled as a button). Primary = --accent, secondary =
 * --surface with a --line border. `size="page"` is the 48 px primary page action.
 */
withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary';
    size?: 'default' | 'page';
    href?: string;
    type?: 'button' | 'submit';
    block?: boolean;
    disabled?: boolean;
  }>(),
  { variant: 'secondary', size: 'default', href: undefined, type: 'button', block: false },
);
</script>

<template>
  <a
    v-if="href"
    :href="href"
    class="btn"
    :class="[`btn--${variant}`, `btn--${size}`, { 'btn--block': block }]"
  >
    <slot />
  </a>
  <button
    v-else
    :type="type"
    :disabled="disabled"
    class="btn"
    :class="[`btn--${variant}`, `btn--${size}`, { 'btn--block': block }]"
  >
    <slot />
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 40px;
  padding: 0 14px;
  border-radius: var(--radius-control);
  border: 1px solid transparent;
  font-family: var(--font-sans);
  font-size: 14px;
  font-weight: 600;
  line-height: 1.2;
  text-decoration: none;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color 120ms ease,
    border-color 120ms ease;
}
.btn--page {
  min-height: 48px;
  font-size: 15px;
}
.btn--block {
  width: 100%;
}
.btn--primary {
  background: var(--accent);
  color: var(--on-accent);
  border-color: var(--accent);
}
.btn--primary.btn--page {
  font-weight: 700;
}
.btn--primary:hover:not(:disabled) {
  background: color-mix(in srgb, var(--accent) 88%, var(--text));
  color: var(--on-accent);
}
.btn--secondary {
  background: var(--surface);
  color: var(--text);
  border-color: var(--line);
}
.btn--secondary:hover:not(:disabled) {
  background: var(--well);
  color: var(--text);
}
.btn:disabled {
  cursor: progress;
  opacity: 0.75;
}
</style>
