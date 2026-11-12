<script setup lang="ts">
import { computed } from 'vue';

import { avatarColorIndex, initials } from '@/lib/avatar';

const props = withDefaults(
  defineProps<{ userId: string; name: string; size?: number; ring?: string }>(),
  { size: 32, ring: 'var(--surface)' },
);

const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  fontSize: `${Math.round(props.size * 0.4)}px`,
  background: `var(--avatar-${avatarColorIndex(props.userId)})`,
  boxShadow: `0 0 0 2px ${props.ring}`,
}));
</script>

<template>
  <span class="avatar" :style="style" aria-hidden="true">{{ initials(name) }}</span>
</template>

<style scoped>
.avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  border-radius: 50%;
  color: var(--on-avatar);
  font-weight: 700;
  letter-spacing: 0.02em;
  user-select: none;
}
</style>
