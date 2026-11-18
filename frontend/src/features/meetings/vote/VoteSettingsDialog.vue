<script setup lang="ts">
/**
 * Vote settings (VoteSettings artboard, spec §9.4): the facilitator sets
 * the votes per person and the most votes per group. Lower limits remove
 * people's most recent votes; the dialog says how many people that touches,
 * never who: the server counts them (the client never sees anyone's usage),
 * and Save waits for that count.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import { useErrorText } from '@/composables/useErrorText';
import { MeetingPhase } from '@/api/client';
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
/**
 * Bumped each time the dialog opens: a save or a count from an earlier
 * opening neither closes nor changes this one.
 */
let session = 0;

/** A whole number, nothing else (`1x` is not 1). */
const WHOLE = /^\s*\d+\s*$/;
const budgetValid = computed(() => WHOLE.test(budgetText.value));
const perGroupValid = computed(() => WHOLE.test(perGroupText.value));

/** People the draft limits would take votes from, for the draft it was asked for. */
const impact = ref<{ key: string; affected: number | null } | null>(null);
let impactTimer: ReturnType<typeof setTimeout> | undefined;

function reset(): void {
  session++;
  budget.value = store.voteBudget;
  perGroup.value = store.voteMaxPerGroup;
  budgetText.value = String(budget.value);
  perGroupText.value = String(perGroup.value);
  error.value = null;
  busy.value = false;
  impact.value = null;
}

watch(
  () => props.open,
  (open) => {
    if (open) reset();
    else session++;
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
 * follows the keyboard (and never moves Save as the field is left). Anything
 * but a whole number is left as typed, and Save waits for a valid one.
 */
function onInput(which: 'budget' | 'perGroup'): void {
  const text = which === 'budget' ? budgetText : perGroupText;
  const value = which === 'budget' ? budget : perGroup;
  if (!WHOLE.test(text.value)) return;
  value.value = clamp(Number.parseInt(text.value, 10), which === 'budget' ? BUDGET : PER_GROUP);
}

/** Shows the value taken once the field is left (a valid one only). */
function commit(which: 'budget' | 'perGroup'): void {
  const text = which === 'budget' ? budgetText : perGroupText;
  if (!WHOLE.test(text.value)) return;
  onInput(which);
  text.value = String(which === 'budget' ? budget.value : perGroup.value);
}

/** Whether the draft lowers a limit while votes exist (Vote): votes may go. */
const lowers = computed(
  () =>
    store.meeting?.phase === MeetingPhase.vote &&
    (budget.value < store.voteBudget || perGroup.value < store.voteMaxPerGroup),
);
const draftKey = computed(() => `${budget.value}/${perGroup.value}`);
/** The count for the current draft, once the server gave it. */
const current = computed(() =>
  impact.value && impact.value.key === draftKey.value ? impact.value : null,
);
const affected = computed(() => (lowers.value ? (current.value?.affected ?? 0) : 0));
/** Lower limits wait for the count; a count the server could not give warns anyway. */
const impactKnown = computed(() => !lowers.value || current.value !== null);
const impactUnknown = computed(() => lowers.value && current.value?.affected === null);

watch(
  [() => props.open, lowers, draftKey],
  () => {
    clearTimeout(impactTimer);
    if (!props.open || !lowers.value) return;
    const key = draftKey.value;
    const mine = session;
    const [b, m] = [budget.value, perGroup.value];
    impactTimer = setTimeout(async () => {
      const count = await store.voteImpact(b, m);
      if (mine === session) impact.value = { key, affected: count };
    }, 200);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(impactTimer));

const canSave = computed(
  () => !busy.value && budgetValid.value && perGroupValid.value && impactKnown.value,
);

async function save(): Promise<void> {
  commit('budget');
  commit('perGroup');
  if (!canSave.value) return;
  const mine = session;
  busy.value = true;
  error.value = null;
  try {
    const saved = await store.saveVoteSettings(budget.value, perGroup.value);
    if (mine !== session) return;
    if (saved) emit('close');
    else error.value = store.actionError;
  } finally {
    if (mine === session) busy.value = false;
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
    <p v-if="!budgetValid || !perGroupValid" class="invalid" role="alert">
      {{ t('vote.wholeNumber') }}
    </p>
    <div v-if="affected || impactUnknown" class="warning" role="status" data-testid="vote-warning">
      <AppIcon name="alert" :size="18" />
      <span>
        <template v-if="impactUnknown">{{ t('vote.warnUnknown') }}</template>
        <template v-else>{{ t('vote.warnAffected', { count: affected }, affected) }}</template>
        {{ t('vote.warnTrim') }}
      </span>
    </div>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('vote.cancel') }}</AppButton>
      <AppButton variant="primary" :disabled="!canSave" @click="save">{{
        t('vote.save')
      }}</AppButton>
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
.invalid {
  margin: 0;
  color: var(--danger);
  font-size: 13px;
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
