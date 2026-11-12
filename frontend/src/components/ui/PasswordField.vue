<script setup lang="ts">
import { ref } from 'vue';

import TextField from './TextField.vue';

defineOptions({ inheritAttrs: false });
defineProps<{ id: string; label: string; showLabel: string; hideLabel: string }>();
const model = defineModel<string>({ required: true });
const visible = ref(false);
</script>

<template>
  <TextField
    :id="id"
    v-model="model"
    :label="label"
    :type="visible ? 'text' : 'password'"
    v-bind="$attrs"
  >
    <template #aside><slot name="aside" /></template>
    <template #trailing>
      <button
        type="button"
        class="toggle"
        :aria-label="visible ? hideLabel : showLabel"
        :aria-pressed="visible"
        :aria-controls="id"
        @click="visible = !visible"
      >
        <svg
          v-if="!visible"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path
            d="M9.9 4.2A10 10 0 0 1 12 4c7 0 10 8 10 8a17 17 0 0 1-2.2 3.3M6.6 6.6C3.6 8.6 2 12 2 12s3 8 10 8a9.7 9.7 0 0 0 5.4-1.6"
          />
          <path d="M2 2l20 20" />
          <path d="M9.9 9.9a3 3 0 0 0 4.2 4.2" />
        </svg>
        <svg
          v-else
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M2 12s3-8 10-8 10 8 10 8-3 8-10 8S2 12 2 12z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      </button>
    </template>
  </TextField>
</template>

<style scoped>
.toggle {
  position: absolute;
  top: 3px;
  right: 3px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.toggle:hover {
  color: var(--text);
}
</style>
