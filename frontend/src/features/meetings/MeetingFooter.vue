<script setup lang="ts">
/**
 * The meeting room's bottom bar (Icebreaker and EndMeeting artboards): where
 * the meeting stands, and the facilitator's Next. In the icebreaker that is
 * Skip / Next person, then "Next: Reflect" on the last person; after the last
 * phase it is End meeting.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { MeetingPhase } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { firstName } from '@/features/meetings/firstName';
import { useMeetingStore } from '@/stores/meeting';

const emit = defineEmits<{ end: [] }>();
const { t } = useI18n();
const store = useMeetingStore();
const meeting = computed(() => store.meeting!);
const inIcebreaker = computed(() => meeting.value.phase === MeetingPhase.icebreaker);
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
    <span class="bar__status">
      <AppIcon name="users" :size="16" />
      <template v-if="inIcebreaker && speaker">
        {{
          t('icebreaker.status', {
            name: firstName(speaker.displayName),
            n: store.speakerIndex + 1,
            total: store.speakers.length,
          })
        }}
      </template>
      <template v-else>{{ t('meetingRoom.followFacilitator') }}</template>
    </span>
    <span class="bar__spacer"></span>
    <div v-if="controls" class="bar__actions">
      <template v-if="inIcebreaker && speaker && !store.isLastSpeaker">
        <AppButton size="page" :disabled="busy" @click="run(() => store.nextPerson(true))">
          <AppIcon name="skip" :size="16" />
          {{ t('icebreaker.skip', { name: firstName(speaker.displayName) }) }}
        </AppButton>
        <AppButton
          class="bar__next"
          variant="primary"
          size="page"
          :disabled="busy"
          @click="run(() => store.nextPerson(false))"
        >
          {{ t('icebreaker.nextPerson') }}
          <AppIcon name="arrowRight" :size="16" />
        </AppButton>
      </template>
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
.bar__status {
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
