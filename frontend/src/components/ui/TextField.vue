<script setup lang="ts">
defineOptions({ inheritAttrs: false });
defineProps<{ id: string; label: string }>();
const model = defineModel<string>({ required: true });
</script>

<template>
  <div class="field">
    <div class="field__header">
      <label :for="id" class="field__label">{{ label }}</label>
      <slot name="aside" />
    </div>
    <div class="field__control">
      <input
        :id="id"
        v-model="model"
        class="field__input"
        :class="{ 'field__input--trailing': !!$slots.trailing }"
        v-bind="$attrs"
      />
      <slot name="trailing" />
    </div>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field__header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}
.field__label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}
.field__control {
  position: relative;
}
.field__input {
  width: 100%;
  height: 46px;
  padding: 0 14px;
  border-radius: var(--radius-control);
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 15px;
}
.field__input::placeholder {
  color: var(--muted);
  opacity: 0.8;
}
.field__input:focus-visible {
  border-color: var(--accent);
}
.field__input[aria-invalid='true'] {
  border-color: var(--danger);
}
.field__input--trailing {
  padding-right: 48px;
}
</style>
