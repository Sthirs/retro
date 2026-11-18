<script setup lang="ts">
/**
 * Vote settings (VoteSettings artboard, spec §9.4): the facilitator sets
 * the votes per person and the most votes per group. Lower limits remove
 * people's most recent votes; the dialog says how many people that touches,
 * never who.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import { useErrorText } from '@/composables/useErrorText';
import { useMeetingStore } from '@/stores/meeting';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const errorText = useErrorText();
const store = useMeetingStore();

/** Defaults and bounds (spec §9.4). */
const BUDGET = { default: 5, min: 1, max: 20 };
const PER_GROUP = { default: 3, min: 1, max: 10 };

const budget = ref(BUDGET.default);
const perGroup = ref(PER_GROUP.default);
const budgetText = ref('');
const perGroupText = ref('');
const busy = ref(false);
const error = ref<string | null>(null);

function reset(): void {
  budget.value = store.voteBudget;
  perGroup.value = store.voteMaxPerGroup;
  budgetText.value = String(budget.value);
  perGroupText.value = String(perGroup.value);
  error.value = null;
}

watch(
  () => props.open,
  (open) => {
    if (open) reset();
  },
);

function clamp(value: number, bounds: { min: number; max: number }): number {
  return Math.min(bounds.max, Math.max(bounds.min, value));
}

function step(which: 'budget' | 'perGroup', delta: number): void {
  if (which === 'budget') {
    budget.value = clamp(budget.value + delta, BUDGET);
    budgetText.value = String(budget.value);
  } else {
    perGroup.value = clamp(perGroup.value + delta, PER_GROUP);
    perGroupText.value = String(perGroup.value);
  }
}

/**
 * Takes a typed number as it is typed, clamped to its bounds, so the warning
 * follows the keyboard (and never moves Save as the field is left).
 */
function onInput(which: 'budget' | 'perGroup'): void {
  const text = which === 'budget' ? budgetText : perGroupText;
  const value = which === 'budget' ? budget : perGroup;
  const parsed = Number.parseInt(text.value.trim(), 10);
  if (!Number.isNaN(parsed)) value.value = clamp(parsed, which === 'budget' ? BUDGET : PER_GROUP);
}

/** Shows the value taken once the field is left. */
function commit(which: 'budget' | 'perGroup'): void {
  onInput(which);
  if (which === 'budget') budgetText.value = String(budget.value);
  else perGroupText.value = String(perGroup.value);
}

/** People over the new limits, anonymous (from the facilitator's usage). */
const overBudget = computed(
  () => (store.meeting?.voteUsage ?? []).filter((u) => u.spent > budget.value).length,
);
const overPerGroup = computed(
  () => (store.meeting?.voteUsage ?? []).filter((u) => u.maxOnGroup > perGroup.value).length,
);

async function save(): Promise<void> {
  commit('budget');
  commit('perGroup');
  if (busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    if (await store.saveVoteSettings(budget.value, perGroup.value)) emit('close');
    else error.value = store.actionError;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('vote.settingsTitle')" :width="480" @close="emit('close')">
    <AppAlert v-if="error">{{ errorText(error) }}</AppAlert>
    <div class="setting">
      <div class="setting__text">
        <label for="vote-budget" class="setting__label">{{ t('vote.budgetLabel') }}</label>
        <span class="setting__default">{{ t('vote.default', { n: BUDGET.default }) }}</span>
      </div>
      <div class="number">
        <button
          type="button"
          class="number__button"
          :aria-label="`${t('vote.decrease')}: ${t('vote.budgetLabel')}`"
          :disabled="budget <= BUDGET.min"
          @click="step('budget', -1)"
        >
          <AppIcon name="minus" :size="16" />
        </button>
        <input
          id="vote-budget"
          v-model="budgetText"
          class="number__input"
          type="text"
          inputmode="numeric"
          autocomplete="off"
          @input="onInput('budget')"
          @blur="commit('budget')"
          @keydown.enter.prevent="commit('budget')"
        />
        <button
          type="button"
          class="number__button"
          :aria-label="`${t('vote.increase')}: ${t('vote.budgetLabel')}`"
          :disabled="budget >= BUDGET.max"
          @click="step('budget', 1)"
        >
          <AppIcon name="plus" :size="16" />
        </button>
      </div>
    </div>
    <div class="setting">
      <div class="setting__text">
        <label for="vote-per-group" class="setting__label">{{ t('vote.maxLabel') }}</label>
        <span class="setting__default">{{ t('vote.default', { n: PER_GROUP.default }) }}</span>
      </div>
      <div class="number">
        <button
          type="button"
          class="number__button"
          :aria-label="`${t('vote.decrease')}: ${t('vote.maxLabel')}`"
          :disabled="perGroup <= PER_GROUP.min"
          @click="step('perGroup', -1)"
        >
          <AppIcon name="minus" :size="16" />
        </button>
        <input
          id="vote-per-group"
          v-model="perGroupText"
          class="number__input"
          type="text"
          inputmode="numeric"
          autocomplete="off"
          @input="onInput('perGroup')"
          @blur="commit('perGroup')"
          @keydown.enter.prevent="commit('perGroup')"
        />
        <button
          type="button"
          class="number__button"
          :aria-label="`${t('vote.increase')}: ${t('vote.maxLabel')}`"
          :disabled="perGroup >= PER_GROUP.max"
          @click="step('perGroup', 1)"
        >
          <AppIcon name="plus" :size="16" />
        </button>
      </div>
    </div>
    <div v-if="overBudget || overPerGroup" class="warning" role="status" data-testid="vote-warning">
      <AppIcon name="alert" :size="18" />
      <span>
        <template v-if="overBudget">
          {{ t('vote.warnBudget', { count: overBudget, n: budget }, overBudget) }}
        </template>
        <template v-if="overPerGroup">
          {{ t('vote.warnMax', { count: overPerGroup, n: perGroup }, overPerGroup) }}
        </template>
        {{ t('vote.warnTrim') }}
      </span>
    </div>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('vote.cancel') }}</AppButton>
      <AppButton variant="primary" :disabled="busy" @click="save">{{ t('vote.save') }}</AppButton>
    </template>
  </AppDialog>
</template>

<style scoped>
.setting {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.setting__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.setting__label {
  font-size: 14px;
  font-weight: 600;
}
.setting__default {
  color: var(--muted);
  font-size: 12px;
}
.number {
  display: flex;
  flex: none;
  align-items: center;
  overflow: hidden;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
}
.number__button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--text);
  cursor: pointer;
}
.number__button:hover:not(:disabled) {
  background: var(--well);
}
.number__button:disabled {
  color: var(--muted);
  opacity: 0.45;
  cursor: default;
}
.number__input {
  width: 48px;
  height: 44px;
  box-sizing: border-box;
  border: none;
  border-right: 1px solid var(--line);
  border-left: 1px solid var(--line);
  background: transparent;
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 15px;
  font-weight: 700;
  text-align: center;
}
.warning {
  display: flex;
  gap: 10px;
  padding: 12px 14px;
  border-radius: var(--radius-control);
  background: var(--col-delta-bg);
  color: var(--text);
  font-size: 13px;
  line-height: 1.45;
}
.warning :deep(svg) {
  flex: none;
  color: var(--col-delta-fg);
}
</style>
