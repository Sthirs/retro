<script setup lang="ts">
/**
 * The meeting room's left column (Icebreaker artboard): Leave, the title
 * (renamable by the facilitator), the facilitator, and the phase stepper with
 * the icebreaker's speaking order under the current phase.
 */
import { computed, nextTick, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink } from 'vue-router';

import { MeetingPhase } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import PresenceAvatar from '@/components/PresenceAvatar.vue';
import RetroLogo from '@/components/RetroLogo.vue';
import { PHASE_ICONS } from '@/features/meetings/phaseIcons';
import { formatDate } from '@/lib/time';
import { PHASES, useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

const { t } = useI18n();
const store = useMeetingStore();
const session = useSessionStore();
const meeting = computed(() => store.meeting!);
const me = computed(() => session.user?.id ?? null);

const nameOf = (id: string, name: string) =>
  id === me.value ? t('meetingRoom.you', { name }) : name;

// The facilitator goes back to anyone who already had their turn.
const canGoBack = computed(() => store.isFacilitator && store.isActive);
const going = ref(false);
async function goTo(userId: string): Promise<void> {
  if (going.value) return;
  going.value = true;
  try {
    await store.goToPerson(userId);
  } finally {
    going.value = false;
  }
}

type StepState = 'done' | 'current' | 'upcoming';
const steps = computed(() =>
  PHASES.map((phase, i) => ({
    phase,
    state: (i < store.phaseIndex
      ? 'done'
      : i === store.phaseIndex
        ? 'current'
        : 'upcoming') as StepState,
  })),
);

type SpeakerState = 'done' | 'current' | 'next' | 'upcoming';
const speakers = computed(() =>
  store.speakers.map((p, i) => {
    let state: SpeakerState = 'upcoming';
    if (i < store.speakerIndex) state = 'done';
    else if (i === store.speakerIndex) state = 'current';
    else if (i === store.speakerIndex + 1) state = 'next';
    return { participant: p, state, back: canGoBack.value && state === 'done' };
  }),
);

// Inline rename (facilitator).
const editing = ref(false);
const draft = ref('');
const input = ref<HTMLInputElement | null>(null);
const renameButton = ref<HTMLButtonElement | null>(null);

async function startRename(): Promise<void> {
  draft.value = meeting.value.title;
  editing.value = true;
  await nextTick();
  input.value?.select();
}

async function finishRename(save: boolean): Promise<void> {
  if (!editing.value) return;
  editing.value = false;
  const title = draft.value.trim();
  if (save && title && title !== meeting.value.title) await store.rename(title);
  await nextTick();
  renameButton.value?.focus();
}
</script>

<template>
  <nav class="side" :aria-label="t('meetingRoom.phasesLabel')">
    <div class="side__top">
      <span class="brand">
        <RetroLogo :size="28" />
        <span class="brand__wordmark">retro</span>
      </span>
      <RouterLink class="leave" :to="{ name: 'team-meetings', params: { teamId: meeting.teamId } }">
        <AppIcon name="logout" :size="16" />
        {{ t('meetingRoom.leave') }}
      </RouterLink>
    </div>

    <div class="side__meeting">
      <form v-if="editing" class="rename" @submit.prevent="finishRename(true)">
        <label class="visually-hidden" for="meeting-title">{{ t('meetingRoom.titleLabel') }}</label>
        <input
          id="meeting-title"
          ref="input"
          v-model="draft"
          class="rename__input"
          maxlength="80"
          @keydown.esc.prevent="finishRename(false)"
          @blur="finishRename(true)"
        />
      </form>
      <span v-else class="side__title">
        {{ meeting.title }}
        <button
          v-if="store.isFacilitator && store.isActive"
          ref="renameButton"
          type="button"
          class="rename__button"
          :aria-label="t('meetingRoom.rename')"
          @click="startRename"
        >
          <AppIcon name="pencil" :size="16" />
        </button>
      </span>
      <span class="side__sub">
        {{
          t('meetingRoom.teamDate', { team: meeting.teamName, date: formatDate(meeting.createdAt) })
        }}
      </span>
    </div>

    <div v-if="store.facilitator" class="side__facilitator">
      <PresenceAvatar
        :user-id="store.facilitator.id"
        :name="store.facilitator.displayName"
        :picture-version="store.facilitator.pictureVersion"
        :online="store.facilitator.online"
      />
      <div class="side__facilitator-text">
        <span class="label">{{ t('meetingRoom.facilitator') }}</span>
        <span class="side__facilitator-name">
          {{ nameOf(store.facilitator.id, store.facilitator.displayName) }}
        </span>
      </div>
    </div>

    <ol class="steps">
      <li v-for="(step, i) in steps" :key="step.phase">
        <div
          class="step"
          :class="`step--${step.state}`"
          :aria-current="step.state === 'current' ? 'step' : undefined"
        >
          <AppIcon :name="PHASE_ICONS[step.phase]" />
          {{ t(`phases.${step.phase}`) }}
          <span v-if="step.state === 'current'" class="step__count">
            {{ t('meetingRoom.stepCount', { n: i + 1, total: PHASES.length }) }}
          </span>
          <span
            v-else-if="step.state === 'done'"
            class="step__done"
            :aria-label="t('meetingRoom.completed')"
          >
            <AppIcon name="check" :size="16" />
          </span>
        </div>
        <ol
          v-if="step.state === 'current' && step.phase === MeetingPhase.icebreaker"
          class="queue"
          :aria-label="t('icebreaker.progress')"
        >
          <li
            v-for="{ participant, state, back } in speakers"
            :key="participant.id"
            class="queue__entry"
            :aria-current="state === 'current' ? 'true' : undefined"
          >
            <component
              :is="back ? 'button' : 'div'"
              class="queue__item"
              :class="`queue__item--${state}`"
              :type="back ? 'button' : undefined"
              :disabled="back ? going : undefined"
              :aria-label="
                back ? t('icebreaker.goTo', { name: participant.displayName }) : undefined
              "
              @click="back && goTo(participant.id)"
            >
              <PresenceAvatar
                :user-id="participant.id"
                :name="participant.displayName"
                :picture-version="participant.pictureVersion"
                :online="participant.online"
                :size="26"
                ring="var(--accent-soft)"
              />
              <span class="queue__name">{{ nameOf(participant.id, participant.displayName) }}</span>
              <span v-if="state === 'done'" class="queue__done" :aria-label="t('icebreaker.done')">
                <AppIcon name="check" :size="16" />
              </span>
              <span v-else-if="state === 'current'" class="queue__now">{{
                t('icebreaker.now')
              }}</span>
              <span v-else-if="state === 'next'" class="queue__next">{{
                t('icebreaker.next')
              }}</span>
            </component>
          </li>
        </ol>
      </li>
    </ol>
  </nav>
</template>

<style scoped>
.side {
  display: flex;
  flex: 1 1 272px;
  flex-direction: column;
  max-width: 100%;
  box-sizing: border-box;
  background: var(--surface);
  border-right: 1px solid var(--line);
}
.side__top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 14px 12px 14px 16px;
  border-bottom: 1px solid var(--line);
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.brand__wordmark {
  font-size: 19px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.leave {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 36px;
  padding: 0 10px;
  border-radius: var(--radius-control);
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
  text-decoration: none;
}
.leave:hover {
  background: var(--well);
  color: var(--text);
}
.side__meeting {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 16px 16px 14px;
  border-bottom: 1px solid var(--line);
}
.side__title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 20px;
  font-weight: 700;
  overflow-wrap: anywhere;
}
.rename__button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 32px;
  min-height: 32px;
  padding: 4px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--muted);
  cursor: pointer;
}
.rename__button:hover {
  background: var(--well);
  color: var(--text);
}
.rename__input {
  width: 100%;
  min-height: 40px;
  box-sizing: border-box;
  padding: 0 10px;
  border: 1px solid var(--accent);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  font-size: 18px;
  font-weight: 700;
}
.side__sub {
  color: var(--muted);
  font-size: 13px;
}
.side__facilitator {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
}
.side__facilitator-text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
}
.label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.side__facilitator-name {
  font-size: 14px;
  font-weight: 700;
}
.steps {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 12px;
  list-style: none;
}
.step {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 44px;
  padding: 0 12px;
  border-radius: var(--radius-control);
  color: var(--muted);
  font-size: 14px;
  font-weight: 700;
}
.step--done {
  color: var(--text);
}
.step--current {
  background: var(--accent-soft);
  color: var(--text);
  box-shadow: inset 3px 0 0 var(--accent);
}
.step__count {
  margin-left: auto;
  color: var(--accent-text);
  font-size: 12px;
  font-weight: 700;
}
.step__done {
  display: flex;
  margin-left: auto;
  color: var(--success);
}
.queue {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 6px 0 4px;
  padding: 6px;
  border-radius: var(--radius-control);
  background: var(--accent-soft);
  list-style: none;
}
.queue__item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 40px;
  box-sizing: border-box;
  padding: 4px 8px;
  border-radius: var(--radius-sm);
}
button.queue__item {
  width: 100%;
  min-height: 44px;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
button.queue__item:hover:not(:disabled) {
  background: var(--surface);
}
button.queue__item:hover:not(:disabled) .queue__name {
  color: var(--text);
}
.queue__name {
  flex: 1 1 auto;
  min-width: 0;
  color: var(--muted);
  font-size: 13px;
  font-weight: 500;
  line-height: 1.3;
}
.queue__item--next .queue__name {
  color: var(--text);
}
.queue__item--current {
  background: var(--surface);
  box-shadow: var(--shadow-card);
}
.queue__item--current .queue__name {
  color: var(--text);
  font-weight: 700;
}
.queue__done {
  display: flex;
  color: var(--success);
}
.queue__now {
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}
.queue__next {
  color: var(--accent-text);
  font-size: 11px;
  font-weight: 700;
}
</style>
