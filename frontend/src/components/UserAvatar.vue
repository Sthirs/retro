<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import { avatarColorIndex, initials } from '@/lib/avatar';
import { loadImage } from '@/lib/images';

const props = withDefaults(
  defineProps<{
    userId: string;
    name: string;
    size?: number;
    ring?: string;
    /** Content hash of the uploaded picture; initials are shown when `null` or unset. */
    pictureVersion?: string | null;
  }>(),
  { size: 32, ring: 'var(--surface)', pictureVersion: null },
);

const style = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  fontSize: `${Math.round(props.size * 0.4)}px`,
  background: `var(--avatar-${avatarColorIndex(props.userId)})`,
  boxShadow: `0 0 0 2px ${props.ring}`,
}));

/** Object URL of the loaded picture; `null` while loading, on error or without a picture. */
const pictureSrc = ref<string | null>(null);

watch(
  () => [props.userId, props.pictureVersion] as const,
  async ([userId, version]) => {
    pictureSrc.value = null;
    if (!version) return;
    try {
      const src = await loadImage(
        `/api/v1/users/${userId}/picture?v=${encodeURIComponent(version)}`,
      );
      // Ignore a result that a newer userId/version has already superseded.
      if (userId === props.userId && version === props.pictureVersion) pictureSrc.value = src;
    } catch {
      // Keep the initials.
    }
  },
  { immediate: true },
);
</script>

<template>
  <span class="avatar" :style="style" aria-hidden="true">
    <img v-if="pictureSrc" class="avatar__img" :src="pictureSrc" alt="" />
    <template v-else>{{ initials(name) }}</template>
  </span>
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
  overflow: hidden;
}

.avatar__img {
  width: 100%;
  height: 100%;
  border-radius: 50%;
  object-fit: cover;
}
</style>
