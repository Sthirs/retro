import { onBeforeUnmount, ref, watch, type Ref } from 'vue';

/**
 * Open state for a popover anchored to `root`: a pointer press outside or Esc
 * closes it, and Esc returns focus to `trigger`.
 */
export function usePopover(
  root: Ref<HTMLElement | null>,
  trigger: Ref<HTMLElement | null>,
): { open: Ref<boolean>; toggle: () => void; close: (refocus?: boolean) => void } {
  const open = ref(false);

  function close(refocus = false): void {
    if (!open.value) return;
    open.value = false;
    if (refocus) trigger.value?.focus();
  }

  function toggle(): void {
    open.value = !open.value;
  }

  function onPointerDown(event: PointerEvent): void {
    if (root.value && !root.value.contains(event.target as Node)) close();
  }

  function onKeyDown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      close(true);
    }
  }

  watch(open, (isOpen) => {
    if (isOpen) {
      document.addEventListener('pointerdown', onPointerDown, true);
      root.value?.addEventListener('keydown', onKeyDown);
    } else {
      document.removeEventListener('pointerdown', onPointerDown, true);
      root.value?.removeEventListener('keydown', onKeyDown);
    }
  });

  onBeforeUnmount(() => {
    document.removeEventListener('pointerdown', onPointerDown, true);
    root.value?.removeEventListener('keydown', onKeyDown);
  });

  return { open, toggle, close };
}
