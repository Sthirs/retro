<script setup lang="ts">
/**
 * The meeting room (spec §8.2): joins the meeting, follows its channel and
 * shows the current phase. Everyone sees the facilitator's changes live; when
 * the meeting ends, everyone lands on the ended view.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute } from 'vue-router';

import { MeetingPhase } from '@/api/client';
import { realtime } from '@/api/realtime';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useChannel } from '@/composables/useChannel';
import { useErrorText } from '@/composables/useErrorText';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import EndMeetingDialog from '@/features/meetings/EndMeetingDialog.vue';
import MeetingEndedView from '@/features/meetings/MeetingEndedView.vue';
import MeetingFooter from '@/features/meetings/MeetingFooter.vue';
import MeetingSidebar from '@/features/meetings/MeetingSidebar.vue';
import PhaseHeader from '@/features/meetings/PhaseHeader.vue';
import ReconnectingBanner from '@/features/meetings/ReconnectingBanner.vue';
import TransferDialog from '@/features/meetings/TransferDialog.vue';
import IcebreakerPhase from '@/features/meetings/phases/IcebreakerPhase.vue';
import PhasePlaceholder from '@/features/meetings/phases/PhasePlaceholder.vue';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

const { t } = useI18n();
const errorText = useErrorText();
const route = useRoute();
const store = useMeetingStore();
const session = useSessionStore();

const meetingId = computed(() => String(route.params.meetingId));
const teamId = computed(() => String(route.params.teamId));
const meeting = computed(() =>
  store.meeting && store.meeting.id === meetingId.value && store.meeting.teamId === teamId.value
    ? store.meeting
    : null,
);
const transferOpen = ref(false);
const endOpen = ref(false);
const offline = computed(
  () => realtime.status.value === 'reconnecting' || realtime.status.value === 'connecting',
);

// Subscribe first, then load: events that arrive before the snapshot are queued.
useChannel(
  () => `meeting:${meetingId.value}`,
  (event) => store.apply(event),
);
watch(meetingId, (id) => void store.open(id), { immediate: true });
onBeforeUnmount(() => store.reset());

// Ending closes the dialogs for everyone: the ended view replaces the room.
watch(
  () => store.isActive,
  (active) => {
    if (!active) {
      transferOpen.value = false;
      endOpen.value = false;
    }
  },
);
// Losing the facilitator role closes the facilitator's dialogs.
watch(
  () => store.isFacilitator,
  (facilitator) => {
    if (!facilitator) {
      transferOpen.value = false;
      endOpen.value = false;
    }
  },
);

/** "Marco Bianchi is now the facilitator", for a few seconds. */
const noticeText = ref('');
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => store.notice,
  (notice) => {
    if (!notice) return;
    noticeText.value =
      notice.userId === session.user?.id
        ? t('meetingRoom.youFacilitate')
        : t('meetingRoom.facilitatorChanged', { name: notice.name });
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (noticeText.value = ''), 6000);
  },
);
onBeforeUnmount(() => clearTimeout(noticeTimer));
</script>

<template>
  <LinkLayout v-if="!meeting && store.loadError">
    <LinkStateCard
      icon="alert"
      :title="t('meetingRoom.notFound')"
      :text="errorText(store.loadError)"
    >
      <AppButton :to="{ name: 'team-meetings', params: { teamId } }">
        {{ t('meetingRoom.backToMeetings') }}
      </AppButton>
    </LinkStateCard>
  </LinkLayout>
  <LinkLayout v-else-if="!meeting">
    <p class="loading" aria-busy="true">{{ t('meetingRoom.loading') }}</p>
  </LinkLayout>
  <MeetingEndedView v-else-if="!store.isActive" :meeting="meeting" />
  <div v-else class="room">
    <MeetingSidebar />
    <div class="room__main">
      <ReconnectingBanner v-if="offline" />
      <main class="room__content">
        <PhaseHeader @transfer="transferOpen = true" @end="endOpen = true" />
        <AppAlert v-if="store.actionError">{{ errorText(store.actionError) }}</AppAlert>
        <div class="room__phase">
          <IcebreakerPhase v-if="meeting.phase === MeetingPhase.icebreaker" />
          <PhasePlaceholder v-else />
        </div>
      </main>
      <MeetingFooter @end="endOpen = true" />
    </div>
    <div class="notice" role="status">
      <p v-if="noticeText" class="notice__toast">{{ noticeText }}</p>
    </div>
    <TransferDialog :open="transferOpen" @close="transferOpen = false" />
    <EndMeetingDialog :open="endOpen" @close="endOpen = false" />
  </div>
</template>

<style scoped>
.loading {
  color: var(--muted);
}
.room {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  align-items: stretch;
  min-height: 100vh;
  background: var(--bg);
}
.room__main {
  display: flex;
  flex: 999 1 560px;
  flex-direction: column;
  min-width: 0;
}
.room__content {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 22px;
  box-sizing: border-box;
  padding: 28px 32px 24px;
}
.room__phase {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: 16px;
  min-height: 0;
}
.notice {
  position: fixed;
  right: 24px;
  bottom: 96px;
  z-index: 2;
}
.notice__toast {
  margin: 0;
  padding: 12px 16px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  box-shadow: var(--shadow-menu);
  font-size: 14px;
  font-weight: 600;
}
@media (max-width: 600px) {
  .room__content {
    padding: 20px 16px;
  }
  .notice {
    right: 16px;
    left: 16px;
  }
}
</style>
