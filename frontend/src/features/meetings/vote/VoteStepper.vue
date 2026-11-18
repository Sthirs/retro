<script setup lang="ts">
/**
 * The viewer's own votes on a group (Vote artboard, spec §9.4): − / count / +.
 * Nobody else's votes show. + is off once the budget is spent or the group
 * has the most votes one person may give it; − is off at zero.
 */
import { computed, nextTick, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ groupId: string; name: string }>();
const { t } = useI18n();
const store = useMeetingStore();

const count = computed(() => store.myVotes.get(props.groupId) ?? 0);
const canAdd = computed(
  () => store.isActive && store.votesLeft > 0 && count.value < store.voteMaxPerGroup,
);
const canRemove = computed(() => store.isActive && count.value > 0);

const addButton = ref<HTMLButtonElement | null>(null);
const removeButton = ref<HTMLButtonElement | null>(null);

/**
 * Keeps the keyboard in the stepper: when the focused button turned disabled
 * (or the focus fell off it), moves to the other one if that is enabled.
 * Focus the viewer moved elsewhere stays where it is.
 */
function keepFocus(preferred: HTMLButtonElement | null, other: HTMLButtonElement | null): void {
  const active = document.activeElement;
  const lost = active === null || active === document.body;
  for (const button of [preferred, other]) {
    if (!button || button.disabled) continue;
    if (
      lost ||
      (active instanceof HTMLButtonElement &&
        active.disabled &&
        [addButton.value, removeButton.value].includes(active))
    ) {
      button.focus();
    }
    return;
  }
}

/**
 * Votes, keeping the keyboard in the stepper: a button that turns disabled
 * would drop the focus, so it moves to the other one; and back again if the
 * server refused the vote.
 */
async function change(delta: 1 | -1): Promise<void> {
  const done = store.vote(props.groupId, delta);
  await nextTick();
  const [own, other] = delta > 0 ? [addButton, removeButton] : [removeButton, addButton];
  if (own.value?.disabled && document.activeElement !== other.value) other.value?.focus();
  if (await done) return;
  await nextTick();
  keepFocus(own.value, other.value);
}
</script>

<template>
  <div
    role="group"
    class="stepper"
    :class="{ 'stepper--on': count > 0 }"
    :aria-label="t('vote.yourVotes', { name, count, max: store.voteMaxPerGroup })"
    data-testid="vote-stepper"
  >
    <button
      ref="removeButton"
      type="button"
      class="stepper__button"
      :aria-label="t('vote.remove', { name })"
      :disabled="!canRemove"
      data-testid="vote-remove"
      @click="change(-1)"
    >
      <AppIcon name="minus" :size="18" />
    </button>
    <span class="stepper__count">
      <AppIcon name="vote" :size="16" />
      <span class="stepper__number" data-testid="vote-count">{{ count }}</span>
    </span>
    <button
      ref="addButton"
      type="button"
      class="stepper__button"
      :aria-label="t('vote.add', { name })"
      :disabled="!canAdd"
      data-testid="vote-add"
      @click="change(1)"
    >
      <AppIcon name="plus" :size="18" />
    </button>
  </div>
</template>

<style scoped>
.stepper {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: var(--radius-pill);
  background: var(--card);
}
.stepper--on {
  border-color: var(--accent);
}
.stepper__button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  padding: 0;
  border: none;
  border-radius: var(--radius-pill);
  background: transparent;
  color: var(--text);
  cursor: pointer;
}
.stepper__button:hover:not(:disabled) {
  background: var(--accent-soft);
}
.stepper__button:disabled {
  color: var(--muted);
  opacity: 0.45;
  cursor: default;
}
.stepper__count {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-width: 56px;
  height: 32px;
  box-sizing: border-box;
  padding: 0 10px;
  border-radius: var(--radius-pill);
  color: var(--muted);
}
.stepper--on .stepper__count {
  background: var(--accent);
  color: var(--on-accent);
}
.stepper__number {
  font-size: 16px;
  font-weight: 800;
  font-variant-numeric: tabular-nums;
}
</style>
