<script setup lang="ts">
/** A user's avatar with an online (or offline) dot, as in the meeting screens. */
import UserAvatar from '@/components/UserAvatar.vue';

withDefaults(
  defineProps<{
    userId: string;
    name: string;
    pictureVersion?: string | null;
    size?: number;
    /** Colour of the ring around the avatar and its dot: the background behind it. */
    ring?: string;
    /** Shows the presence dot when set. */
    online?: boolean | null;
  }>(),
  { pictureVersion: null, size: 32, ring: 'var(--surface)', online: null },
);
</script>

<template>
  <span class="presence" :title="name">
    <UserAvatar
      :user-id="userId"
      :name="name"
      :picture-version="pictureVersion"
      :size="size"
      :ring="ring"
    />
    <span
      v-if="online !== null"
      class="presence__dot"
      :class="{ 'presence__dot--online': online }"
      :style="{ borderColor: ring }"
      aria-hidden="true"
    ></span>
  </span>
</template>

<style scoped>
.presence {
  position: relative;
  display: inline-flex;
  flex: none;
}
.presence__dot {
  position: absolute;
  right: -1px;
  bottom: -1px;
  width: 10px;
  height: 10px;
  box-sizing: border-box;
  border: 2px solid var(--surface);
  border-radius: 50%;
  background: var(--muted);
}
.presence__dot--online {
  background: var(--success);
}
</style>
