<script setup lang="ts">
/**
 * The meeting room's bottom bar (Icebreaker, Reflect, Timer, Group and
 * EndMeeting artboards). Left: the timer (the facilitator's controls, or the
 * countdown for everyone else), then where the meeting stands: the ready
 * count for the facilitator, or who is facilitating. Right: the unsent-draft
 * warning, Ready (Reflect, Group, Vote) and the facilitator's Next. In the
 * icebreaker Next is Previous / Next person, then "Next: Reflect" on the last
 * person; after the last phase it is End meeting.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { MeetingPhase } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { firstName } from '@/features/meetings/firstName';
import ReadyButton from '@/features/meetings/footer/ReadyButton.vue';
import ReadyCount from '@/features/meetings/footer/ReadyCount.vue';
import TimerControl from '@/features/meetings/footer/TimerControl.vue';
import TimerReadout from '@/features/meetings/footer/TimerReadout.vue';
import { useMeetingStore } from '@/stores/meeting';
import { useReflectDraftsStore } from '@/stores/reflectDrafts';

const emit = defineEmits<{ end: [] }>();
const { t } = useI18n();
const store = useMeetingStore();
const drafts = useReflectDraftsStore();
const inIcebreaker = computed(() => store.meeting?.phase === MeetingPhase.icebreaker);
const inReflect = computed(() => store.meeting?.phase === MeetingPhase.reflect);
const controls = computed(() => store.isFacilitator && store.isActive);
const speaker = computed(() => store.currentSpeaker);
const busy = ref(false);

async function run(action: () => Promise<boolean>): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  try {
    await action();
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <footer class="bar" :aria-label="t('meetingRoom.navigation')">
    <div class="bar__group">
      <TimerControl v-if="controls" />
      <TimerReadout v-else />
      <span v-if="controls || store.meeting?.timer" class="bar__divider" aria-hidden="true"></span>
      <span v-if="inIcebreaker && speaker" class="bar__status">
        <AppIcon name="users" :size="16" />
        {{
          t('icebreaker.status', {
            name: firstName(speaker.displayName),
            n: store.speakerIndex + 1,
            total: store.speakers.length,
          })
        }}
      </span>
      <ReadyCount v-else-if="controls && store.hasReady" />
      <span v-else-if="!controls && store.hasReady" class="bar__status">
        <AppIcon name="users" :size="16" />
        {{
          t('meetingRoom.facilitating', { name: firstName(store.facilitator?.displayName ?? '') })
        }}
      </span>
      <span v-else class="bar__status">
        <AppIcon name="users" :size="16" />
        {{ t('meetingRoom.followFacilitator') }}
      </span>
    </div>
    <span class="bar__spacer"></span>
    <div class="bar__actions">
      <span v-if="inReflect && drafts.unsent > 0" class="bar__drafts" data-testid="unsent-drafts">
        <AppIcon name="alert" :size="16" />
        {{ t('reflect.unsentDrafts', drafts.unsent) }}
      </span>
      <ReadyButton v-if="store.hasReady && store.isActive" />
      <template v-if="controls">
        <AppButton
          v-if="inIcebreaker && speaker"
          size="page"
          :disabled="busy || !store.previousSpeaker"
          @click="run(() => store.goToPerson(store.previousSpeaker!.id))"
        >
          <AppIcon name="chevronLeft" :size="16" />
          {{ t('icebreaker.previousPerson') }}
        </AppButton>
        <AppButton
          v-if="inIcebreaker && speaker && !store.isLastSpeaker"
          class="bar__next"
          variant="primary"
          size="page"
          :disabled="busy"
          @click="run(store.nextPerson)"
        >
          {{ t('icebreaker.nextPerson') }}
          <AppIcon name="arrowRight" :size="16" />
        </AppButton>
        <AppButton
          v-else-if="store.nextPhase"
          class="bar__next"
          variant="primary"
          size="page"
          :disabled="busy"
          @click="run(store.advancePhase)"
        >
          {{ t('meetingRoom.nextPhase', { phase: t(`phases.${store.nextPhase}`) }) }}
          <AppIcon name="arrowRight" :size="16" />
        </AppButton>
        <AppButton v-else class="bar__next" variant="danger" size="page" @click="emit('end')">
          <AppIcon name="flag" :size="16" />
          {{ t('meetingRoom.end') }}
        </AppButton>
      </template>
    </div>
  </footer>
</template>

<style scoped>
.bar {
  position: sticky;
  bottom: 0;
  z-index: 1;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 16px;
  padding: 12px 32px;
  background: var(--surface);
  border-top: 1px solid var(--line);
  box-shadow: 0 -2px 8px color-mix(in srgb, var(--text) 5%, transparent);
}
.bar__group {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 18px;
}
.bar__divider {
  width: 1px;
  height: 32px;
  background: var(--line);
}
.bar__status,
.bar__drafts {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 13px;
}
.bar__spacer {
  flex: 1 1 0;
}
.bar__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}
.bar__next {
  padding: 0 20px;
  font-size: 15px;
}
@media (max-width: 600px) {
  .bar {
    padding: 12px 16px;
  }
}
</style>
