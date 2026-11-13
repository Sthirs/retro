<script setup lang="ts">
/** A titled panel of the team settings page. */
defineProps<{ id: string; title: string; description?: string }>();
</script>

<template>
  <section class="panel" :aria-labelledby="id">
    <div class="panel__header">
      <h2 :id="id" class="panel__title">{{ title }}</h2>
      <p v-if="description" class="panel__description">{{ description }}</p>
    </div>
    <slot />
  </section>
</template>

<style scoped>
.panel {
  overflow: hidden;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.panel__header {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 18px 20px;
}
.panel__title {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.panel__description {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
}
</style>
