<script setup lang="ts">
import { RouterLink, type RouteLocationRaw } from 'vue-router';

/**
 * Button (or link styled as a button). Primary = --accent, secondary =
 * --surface with a --line border, ghost = transparent, danger = --danger
 * filled, danger-outline = --danger border. Heights: compact 36, default 40,
 * touch 44, page 48 (the primary page action).
 */
withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-outline';
    size?: 'compact' | 'default' | 'touch' | 'page';
    href?: string;
    /** In-app destination: renders a RouterLink. */
    to?: RouteLocationRaw;
    type?: 'button' | 'submit';
    block?: boolean;
    disabled?: boolean;
  }>(),
  {
    variant: 'secondary',
    size: 'default',
    href: undefined,
    to: undefined,
    type: 'button',
    block: false,
  },
);
</script>

<template>
  <RouterLink
    v-if="to"
    :to="to"
    class="btn"
    :class="[`btn--${variant}`, `btn--${size}`, { 'btn--block': block }]"
  >
    <slot />
  </RouterLink>
  <a
    v-else-if="href"
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
.btn--compact {
  min-height: 36px;
  font-size: 13px;
}
.btn--touch {
  min-height: 44px;
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
.btn--ghost {
  background: transparent;
  color: var(--text);
}
.btn--ghost:hover:not(:disabled) {
  background: var(--well);
  color: var(--text);
}
.btn--danger {
  background: var(--danger);
  color: var(--on-danger);
  border-color: var(--danger);
  font-weight: 700;
}
.btn--danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--danger) 88%, var(--text));
  color: var(--on-danger);
}
.btn--danger-outline {
  background: transparent;
  color: var(--danger);
  border-color: var(--danger);
}
.btn--danger-outline:hover:not(:disabled) {
  background: color-mix(in srgb, var(--danger) 10%, transparent);
  color: var(--danger);
}
.btn:disabled {
  cursor: progress;
  opacity: 0.75;
}
</style>
