<script setup lang="ts">
/**
 * Labelled text input. `hint` and `error` are linked through
 * `aria-describedby`; an error also sets `aria-invalid`. `dense` is the 44 px
 * control used in panels and dialogs.
 */
import { computed } from 'vue';

defineOptions({ inheritAttrs: false });
const props = defineProps<{
  id: string;
  label: string;
  hint?: string;
  error?: string;
  dense?: boolean;
}>();
const model = defineModel<string>({ required: true });

const describedBy = computed(
  () =>
    [props.error ? `${props.id}-error` : null, props.hint ? `${props.id}-hint` : null]
      .filter(Boolean)
      .join(' ') || undefined,
);
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
        :class="{ 'field__input--trailing': !!$slots.trailing, 'field__input--dense': dense }"
        :aria-describedby="describedBy"
        :aria-invalid="error ? 'true' : undefined"
        v-bind="$attrs"
      />
      <slot name="trailing" />
    </div>
    <span v-if="error" :id="`${id}-error`" class="field__error">{{ error }}</span>
    <span v-if="hint" :id="`${id}-hint`" class="field__hint">{{ hint }}</span>
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
.field__input--dense {
  height: 44px;
  font-size: 14px;
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
.field__hint {
  font-size: 12px;
  color: var(--muted);
}
.field__error {
  font-size: 12px;
  font-weight: 600;
  color: var(--danger);
}
</style>
