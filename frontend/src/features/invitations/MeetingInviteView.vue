<script setup lang="ts">
/**
 * `/m/:inviteToken`: joins the team if needed, then the running meeting, and
 * goes straight into its room (spec §6.3, LinkStates artboard).
 */
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import { api, type MeetingInviteAcceptedDto } from '@/api/client';
import AppButton from '@/components/ui/AppButton.vue';
import { useErrorText } from '@/composables/useErrorText';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import { useTeamsStore } from '@/stores/teams';

/** How long "You've been added to…" stays before the room opens. */
const JOINED_PAUSE_MS = 1200;

type State =
  | { kind: 'joining' }
  | { kind: 'joined'; meeting: MeetingInviteAcceptedDto; team: string }
  | { kind: 'ended' }
  | { kind: 'error'; code: string };

const { t } = useI18n();
const errorText = useErrorText();
const route = useRoute();
const router = useRouter();
const teams = useTeamsStore();
const state = ref<State>({ kind: 'joining' });

function enter(meeting: MeetingInviteAcceptedDto): Promise<unknown> {
  return router.replace({
    name: 'meeting-room',
    params: { teamId: meeting.teamId, meetingId: meeting.meetingId },
  });
}

// Leaving the page (back button, another link) cancels the trip to the room.
let left = false;
let pause: ReturnType<typeof setTimeout> | undefined;
onBeforeUnmount(() => {
  left = true;
  clearTimeout(pause);
});

onMounted(async () => {
  try {
    const res = await api.acceptMeetingInvite(encodeURIComponent(String(route.params.inviteToken)));
    if (left) return;
    if (res.status === 200) {
      // The room's guard checks membership against the teams store: read it
      // afresh, a load started before the join would not have the team.
      await teams.reload();
      if (left) return;
      if (!res.data.joinedTeam) {
        await enter(res.data);
        return;
      }
      const team = teams.byId(res.data.teamId)?.name ?? '';
      state.value = { kind: 'joined', meeting: res.data, team };
      pause = setTimeout(() => void enter(res.data), JOINED_PAUSE_MS);
    } else if (res.status === 404) {
      state.value = { kind: 'ended' };
    } else {
      state.value = { kind: 'error', code: res.status === 429 ? 'too_many_requests' : 'unknown' };
    }
  } catch {
    state.value = { kind: 'error', code: 'unknown' };
  }
});
</script>

<template>
  <LinkLayout>
    <p v-if="state.kind === 'joining'" class="busy" aria-busy="true">
      {{ t('meetingInvite.joining') }}
    </p>
    <LinkStateCard
      v-else-if="state.kind === 'joined'"
      tone="accent"
      icon="users"
      :title="t('meetingInvite.joiningTitle', { title: state.meeting.title })"
      :text="t('meetingInvite.joiningText', { team: state.team })"
    >
      <div class="progress" role="progressbar" :aria-label="t('meetingInvite.progress')">
        <div class="progress__bar"></div>
      </div>
    </LinkStateCard>
    <LinkStateCard
      v-else-if="state.kind === 'ended'"
      icon="flag"
      :title="t('meetingInvite.endedTitle')"
      :text="t('meetingInvite.endedText')"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>
    <LinkStateCard
      v-else
      icon="alert"
      :title="t('meetingInvite.endedTitle')"
      :text="errorText(state.code)"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>
  </LinkLayout>
</template>

<style scoped>
.busy {
  color: var(--muted);
}
.progress {
  width: 100%;
  height: 6px;
  overflow: hidden;
  border-radius: var(--radius-pill);
  background: var(--well);
}
.progress__bar {
  width: 62%;
  height: 100%;
  background: var(--accent);
  animation: progress 1.2s ease-out forwards;
}
@keyframes progress {
  from {
    width: 10%;
  }
}
@media (prefers-reduced-motion: reduce) {
  .progress__bar {
    animation: none;
  }
}
</style>
