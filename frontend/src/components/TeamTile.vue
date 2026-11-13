<script setup lang="ts">
/** The team's initial on a rounded --accent-soft tile. */
import { computed } from 'vue';

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 40 });

const initial = computed(() => props.name.trim().charAt(0).toUpperCase() || '?');
const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  borderRadius: `${Math.round(props.size * 0.28)}px`,
  fontSize: `${Math.round(props.size * 0.45)}px`,
}));
</script>

<template>
  <span class="tile" :style="style" aria-hidden="true">{{ initial }}</span>
</template>

<style scoped>
.tile {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  background: var(--accent-soft);
  color: var(--accent-text);
  font-weight: 800;
  user-select: none;
}
</style>
