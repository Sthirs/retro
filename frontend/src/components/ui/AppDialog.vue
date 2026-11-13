<script setup lang="ts">
/**
 * Modal dialog on the native <dialog>: focus is trapped and Esc closes it.
 * Focus returns to the element that had it when the dialog opened.
 */
import { nextTick, onBeforeUnmount, ref, useId, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';

const props = withDefaults(defineProps<{ open: boolean; title: string; width?: number }>(), {
  width: 520,
});
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const dialog = ref<HTMLDialogElement | null>(null);
const titleId = `dialog-${useId()}`;
let returnFocus: HTMLElement | null = null;

watch(
  () => props.open,
  async (open) => {
    await nextTick();
    const el = dialog.value;
    if (!el) return;
    if (open && !el.open) {
      returnFocus = document.activeElement as HTMLElement | null;
      el.showModal();
      el.querySelector<HTMLElement>('[autofocus]')?.focus();
    } else if (!open && el.open) {
      el.close();
      returnFocus?.focus();
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => returnFocus?.focus());

function onCancel(event: Event): void {
  event.preventDefault();
  emit('close');
}

function onBackdrop(event: MouseEvent): void {
  if (event.target === dialog.value) emit('close');
}
</script>

<template>
  <dialog
    ref="dialog"
    class="dialog"
    :style="{ maxWidth: `${width}px` }"
    :aria-labelledby="titleId"
    @cancel="onCancel"
    @click="onBackdrop"
  >
    <div v-if="open" class="dialog__panel">
      <div class="dialog__header">
        <slot name="icon" />
        <h2 :id="titleId" class="dialog__title">{{ title }}</h2>
        <button
          type="button"
          class="dialog__close"
          :aria-label="t('common.close')"
          @click="emit('close')"
        >
          <AppIcon name="close" />
        </button>
      </div>
      <div class="dialog__body">
        <slot />
      </div>
      <div v-if="$slots.footer" class="dialog__footer">
        <slot name="footer" />
      </div>
    </div>
  </dialog>
</template>

<style scoped>
.dialog {
  width: calc(100% - 32px);
  padding: 0;
  border: 1px solid var(--line);
  border-radius: 16px;
  background: var(--surface);
  color: var(--text);
  box-shadow: var(--shadow-dialog);
}
.dialog::backdrop {
  background: var(--overlay);
}
.dialog__panel {
  display: flex;
  flex-direction: column;
}
.dialog__header {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 22px 22px 0 24px;
}
.dialog__title {
  flex: 1 1 auto;
  margin: 0;
  padding-top: 6px;
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.dialog__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 36px;
  height: 36px;
  padding: 0;
  border: none;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.dialog__close:hover {
  background: var(--well);
  color: var(--text);
}
.dialog__body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 14px 24px 22px;
}
.dialog__footer {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 10px;
  padding: 16px 24px;
  border-top: 1px solid var(--line);
}
</style>
