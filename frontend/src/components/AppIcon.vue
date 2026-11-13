<script setup lang="ts">
/**
 * Inline stroke icon (24 viewBox, stroke 2, round caps and joins). Decorative:
 * hidden from assistive technology; label the control that contains it.
 */
import { computed } from 'vue';

import { ICONS, type IconName } from '@/components/icons';

const props = withDefaults(defineProps<{ name: IconName; size?: number; strokeWidth?: number }>(), {
  size: 18,
  strokeWidth: 2,
});

type Shape =
  | { kind: 'path'; d: string }
  | { kind: 'rect'; x: number; y: number; w: number; h: number; r: number }
  | { kind: 'circle'; cx: number; cy: number; r: number };

const shapes = computed<Shape[]>(() =>
  ICONS[props.name].map((spec): Shape => {
    if (spec.startsWith('rect:')) {
      const [x = 0, y = 0, w = 0, h = 0, r = 0] = spec.slice(5).split(',').map(Number);
      return { kind: 'rect', x, y, w, h, r };
    }
    if (spec.startsWith('circle:')) {
      const [cx = 0, cy = 0, r = 0] = spec.slice(7).split(',').map(Number);
      return { kind: 'circle', cx, cy, r };
    }
    return { kind: 'path', d: spec };
  }),
);
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
    class="icon"
  >
    <template v-for="(shape, i) in shapes" :key="i">
      <path v-if="shape.kind === 'path'" :d="shape.d" />
      <rect
        v-else-if="shape.kind === 'rect'"
        :x="shape.x"
        :y="shape.y"
        :width="shape.w"
        :height="shape.h"
        :rx="shape.r"
      />
      <circle v-else :cx="shape.cx" :cy="shape.cy" :r="shape.r" />
    </template>
  </svg>
</template>

<style scoped>
.icon {
  flex: none;
}
</style>
