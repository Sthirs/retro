<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 32 });
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 48 48" aria-hidden="true" class="logo">
    <rect width="48" height="48" rx="13" class="logo__tile" />
    <path
      d="M36 24a12 12 0 1 1-4.2-9.1"
      fill="none"
      class="logo__glyph"
      stroke-width="4"
      stroke-linecap="round"
    />
    <polyline
      points="31.4,8.9 31.8,14.9 25.8,15.3"
      fill="none"
      class="logo__glyph"
      stroke-width="4"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
    <circle cx="24" cy="24" r="3.2" class="logo__dot" />
  </svg>
</template>

<style scoped>
.logo {
  flex: none;
}
.logo__tile {
  fill: var(--accent);
}
.logo__glyph {
  stroke: var(--on-accent);
}
.logo__dot {
  fill: var(--on-accent);
}
</style>
