<script setup lang="ts">
/**
 * The Reflect phase (Reflect artboard, spec §9.2): four columns where each
 * participant writes anonymous reflections. Everyone sees their own in full
 * and, for the others, only how many there are and how many are being
 * written.
 */
import { onBeforeUnmount } from 'vue';

import ReflectColumn from '@/features/meetings/reflect/ReflectColumn.vue';
import { COLUMNS } from '@/features/meetings/reflect/columns';
import { useReflectDraftsStore } from '@/stores/reflectDrafts';

const drafts = useReflectDraftsStore();
// The editors go away with the phase: nobody is writing any more.
onBeforeUnmount(() => drafts.close());
</script>

<template>
  <div class="reflect">
    <ReflectColumn v-for="column in COLUMNS" :key="column" :column="column" />
  </div>
</template>

<style scoped>
.reflect {
  display: grid;
  flex: 1 1 auto;
  grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
  gap: 16px;
  align-items: stretch;
}
</style>
