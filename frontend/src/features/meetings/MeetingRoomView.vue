<script setup lang="ts">
/**
 * The meeting room (spec §8.2): joins the meeting, follows its channel and
 * shows the current phase. Everyone sees the facilitator's changes live; when
 * the meeting ends, everyone lands on its summary (spec §8.1, §8.3).
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import { MeetingPhase } from '@/api/client';
import { realtime } from '@/api/realtime';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useChannel } from '@/composables/useChannel';
import { useErrorText } from '@/composables/useErrorText';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import EndMeetingDialog from '@/features/meetings/EndMeetingDialog.vue';
import MeetingFooter from '@/features/meetings/MeetingFooter.vue';
import MeetingSidebar from '@/features/meetings/MeetingSidebar.vue';
import PhaseHeader from '@/features/meetings/PhaseHeader.vue';
import ReconnectingBanner from '@/features/meetings/ReconnectingBanner.vue';
import TransferDialog from '@/features/meetings/TransferDialog.vue';
import TimesUpToast from '@/features/meetings/footer/TimesUpToast.vue';
import DiscussPhase from '@/features/meetings/discuss/DiscussPhase.vue';
import GroupPhase from '@/features/meetings/group/GroupPhase.vue';
import IcebreakerPhase from '@/features/meetings/phases/IcebreakerPhase.vue';
import ReflectPhase from '@/features/meetings/reflect/ReflectPhase.vue';
import VotePhase from '@/features/meetings/vote/VotePhase.vue';
import VoteSettingsDialog from '@/features/meetings/vote/VoteSettingsDialog.vue';
import { PHASES, useMeetingStore } from '@/stores/meeting';
import { useReflectDraftsStore } from '@/stores/reflectDrafts';
import { useSessionStore } from '@/stores/session';

const { t } = useI18n();
const errorText = useErrorText();
const route = useRoute();
const router = useRouter();
const store = useMeetingStore();
const session = useSessionStore();
// Created with the room, so drafts follow the meeting even outside Reflect.
useReflectDraftsStore();

const meetingId = computed(() => String(route.params.meetingId));
const teamId = computed(() => String(route.params.teamId));
const meeting = computed(() =>
  store.meeting && store.meeting.id === meetingId.value && store.meeting.teamId === teamId.value
    ? store.meeting
    : null,
);
const transferOpen = ref(false);
const settingsOpen = ref(false);
const endOpen = ref(false);
// Only once a connection was lost or failed: not while the room first connects.
const offline = computed(() => realtime.status.value === 'reconnecting');

// Subscribe first, then load: events that arrive before the snapshot are queued.
useChannel(
  () => `meeting:${meetingId.value}`,
  (event) => store.apply(event),
);
watch([meetingId, teamId], ([id, team]) => void store.open(id, team), { immediate: true });
onBeforeUnmount(() => store.reset());

// An ended meeting (opened late, or ended while here) shows its summary.
// Opened late, joining already says so: no need to load its state first.
watch(
  () =>
    (meeting.value && !store.isActive) || (store.ended && store.meetingId === meetingId.value)
      ? meetingId.value
      : null,
  (ended) => {
    if (!ended) return;
    transferOpen.value = false;
    endOpen.value = false;
    settingsOpen.value = false;
    void router.replace({
      name: 'meeting-summary',
      params: { teamId: teamId.value, meetingId: ended },
    });
  },
  { immediate: true },
);
// Losing the facilitator role closes the facilitator's dialogs.
watch(
  () => store.isFacilitator,
  (facilitator) => {
    if (!facilitator) {
      transferOpen.value = false;
      endOpen.value = false;
      settingsOpen.value = false;
    }
  },
);

// Vote settings change until Vote ends: the dialog closes after it.
watch(
  () => meeting.value?.phase,
  (phase) => {
    if (phase && PHASES.indexOf(phase) > PHASES.indexOf(MeetingPhase.vote)) {
      settingsOpen.value = false;
    }
  },
);

/** "Marco Bianchi is now the facilitator" (or drafts discarded), for a few seconds. */
const noticeText = ref('');
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => store.notice,
  (notice) => {
    if (!notice) return;
    if (notice.kind === 'draftsDiscarded') {
      noticeText.value = t('reflect.draftsDiscarded', notice.count);
    } else {
      noticeText.value =
        notice.userId === session.user?.id
          ? t('meetingRoom.youFacilitate')
          : t('meetingRoom.facilitatorChanged', { name: notice.name });
    }
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
  <LinkLayout v-else-if="!store.isActive">
    <p class="loading" aria-busy="true">{{ t('summary.loading') }}</p>
  </LinkLayout>
  <div v-else class="room">
    <MeetingSidebar />
    <div class="room__main">
      <ReconnectingBanner v-if="offline" />
      <main class="room__content">
        <PhaseHeader
          @settings="settingsOpen = true"
          @transfer="transferOpen = true"
          @end="endOpen = true"
        />
        <AppAlert v-if="store.actionError">{{ errorText(store.actionError) }}</AppAlert>
        <div class="room__phase">
          <!-- Until the new phase's snapshot comes, the old one is shown. -->
          <template v-if="store.phaseLoading">
            <div v-if="store.loadError" class="phase-wait">
              <AppAlert>{{ errorText(store.loadError) }}</AppAlert>
              <AppButton variant="ghost" size="touch" @click="store.resync()">
                {{ t('common.retry') }}
              </AppButton>
            </div>
            <p v-else class="loading" aria-busy="true" data-testid="phase-loading">
              {{ t('meetingRoom.phaseLoading', { phase: t(`phases.${meeting.phase}`) }) }}
            </p>
          </template>
          <IcebreakerPhase v-else-if="meeting.phase === MeetingPhase.icebreaker" />
          <ReflectPhase v-else-if="meeting.phase === MeetingPhase.reflect" />
          <GroupPhase v-else-if="meeting.phase === MeetingPhase.group" />
          <VotePhase v-else-if="meeting.phase === MeetingPhase.vote" />
          <DiscussPhase v-else />
        </div>
      </main>
      <MeetingFooter @end="endOpen = true" />
    </div>
    <div class="notice" role="status">
      <p v-if="noticeText" class="notice__toast">{{ noticeText }}</p>
    </div>
    <TimesUpToast />
    <TransferDialog :open="transferOpen" @close="transferOpen = false" />
    <EndMeetingDialog :open="endOpen" @close="endOpen = false" />
    <VoteSettingsDialog :open="settingsOpen" @close="settingsOpen = false" />
  </div>
</template>

<style scoped>
.loading {
  color: var(--muted);
}
.phase-wait {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 12px;
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
