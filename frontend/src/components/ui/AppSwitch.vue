<script setup lang="ts">
/** On/off switch (`role="switch"`), labelled by the text next to it. */
import { useId } from 'vue';

defineProps<{ label: string; disabled?: boolean }>();
const model = defineModel<boolean>({ required: true });
const labelId = `switch-${useId()}`;
</script>

<template>
  <div class="switch">
    <button
      type="button"
      role="switch"
      class="switch__track"
      :class="{ 'switch__track--on': model }"
      :aria-checked="model"
      :aria-labelledby="labelId"
      :disabled="disabled"
      @click="model = !model"
    >
      <span class="switch__thumb" />
    </button>
    <span :id="labelId" class="switch__label">{{ label }}</span>
  </div>
</template>

<style scoped>
.switch {
  display: flex;
  align-items: center;
  gap: 10px;
}
.switch__track {
  position: relative;
  flex: none;
  width: 44px;
  height: 26px;
  padding: 0;
  border: none;
  border-radius: var(--radius-pill);
  background: var(--line);
  cursor: pointer;
  transition: background-color 120ms ease;
}
.switch__track::before {
  /* 44 px touch target around the 26 px track. */
  content: '';
  position: absolute;
  inset: -9px 0;
}
.switch__track--on {
  background: var(--accent);
}
.switch__track:disabled {
  cursor: progress;
  opacity: 0.75;
}
.switch__thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--surface);
  transition: transform 120ms ease;
}
.switch__track--on .switch__thumb {
  transform: translateX(18px);
}
.switch__label {
  font-size: 14px;
}
</style>
