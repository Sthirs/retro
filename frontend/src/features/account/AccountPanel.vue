<script setup lang="ts">
/** Section chrome of the account page: a titled panel with an optional description and actions. */
defineProps<{
  id: string;
  title: string;
  description?: string;
  /** Lets popovers (a dropdown list) extend past the panel instead of being clipped. */
  overflowVisible?: boolean;
}>();
</script>

<template>
  <section
    class="panel"
    :class="{ 'panel--overflow-visible': overflowVisible }"
    :aria-labelledby="`${id}-title`"
  >
    <div class="panel__header">
      <div class="panel__heading">
        <h2 :id="`${id}-title`" class="panel__title">{{ title }}</h2>
        <p v-if="description" class="panel__description">{{ description }}</p>
      </div>
      <div v-if="$slots.actions" class="panel__actions"><slot name="actions" /></div>
    </div>
    <slot />
  </section>
</template>

<style scoped>
.panel {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
  overflow: hidden;
}
.panel--overflow-visible {
  overflow: visible;
}
.panel__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px;
}
.panel__heading {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.panel__title {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--text);
}
.panel__description {
  margin: 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--muted);
}
.panel__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
</style>
