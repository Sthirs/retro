<script setup lang="ts">
/**
 * The meeting room's phase header (Icebreaker and Group artboards): the
 * phase, the facilitator's actions (or an admin's Take over), everyone's
 * presence and the Invite button.
 */
import { computed, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, MeetingPhase } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import PresenceAvatar from '@/components/PresenceAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { copyText } from '@/lib/clipboard';
import { PHASES, useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

const emit = defineEmits<{ settings: []; transfer: []; end: [] }>();
const { t } = useI18n();
const store = useMeetingStore();
const session = useSessionStore();
const meeting = computed(() => store.meeting!);
const onlineCount = computed(() => meeting.value.participants.filter((p) => p.online).length);

const inviteStatus = ref('');
/** The Invite button reads "Copied" for a moment after a successful copy. */
const copied = ref(false);
let copiedTimer: ReturnType<typeof setTimeout> | undefined;
async function copyInvite(): Promise<void> {
  inviteStatus.value = '';
  const ok = await api
    .getInviteLink(meeting.value.id)
    .then((res) => res.status === 200 && copyText(res.data.url))
    .catch(() => false);
  inviteStatus.value = t(ok ? 'meetingRoom.inviteCopied' : 'meetingRoom.inviteFailed');
  copied.value = ok;
  clearTimeout(copiedTimer);
  if (ok) copiedTimer = setTimeout(() => (copied.value = false), 2000);
}
onBeforeUnmount(() => clearTimeout(copiedTimer));

const takingOver = ref(false);
/** A team admin takes the facilitator role (spec §8.1). */
async function takeOver(): Promise<void> {
  const me = session.user?.id;
  if (!me) return;
  takingOver.value = true;
  try {
    await store.setFacilitator(me);
  } finally {
    takingOver.value = false;
  }
}
</script>

<template>
  <section class="phase" :aria-label="t('meetingRoom.phaseSection')">
    <div class="phase__text">
      <span class="phase__of">
        {{ t('meetingRoom.phaseOf', { n: store.phaseIndex + 1, total: PHASES.length }) }}
      </span>
      <h1 class="phase__title">{{ t(`phases.${meeting.phase}`) }}</h1>
      <p class="phase__hint">{{ t(`meetingRoom.hints.${meeting.phase}`) }}</p>
    </div>

    <div
      v-if="store.isFacilitator && store.isActive"
      role="toolbar"
      class="toolbar"
      :aria-label="t('meetingRoom.facilitatorActions')"
    >
      <AppButton
        v-if="meeting.phase === MeetingPhase.vote"
        variant="ghost"
        data-testid="vote-settings"
        @click="emit('settings')"
      >
        <AppIcon name="settings" :size="16" />
        {{ t('vote.settings') }}
      </AppButton>
      <AppButton variant="ghost" @click="emit('transfer')">
        <AppIcon name="transfer" :size="16" />
        {{ t('meetingRoom.transfer') }}
      </AppButton>
      <AppButton variant="danger-outline" @click="emit('end')">
        <AppIcon name="flag" :size="16" />
        {{ t('meetingRoom.end') }}
      </AppButton>
    </div>
    <div
      v-else-if="store.canTakeOver"
      role="toolbar"
      class="toolbar"
      :aria-label="t('meetingRoom.adminActions')"
    >
      <AppButton size="touch" :disabled="takingOver" @click="takeOver">
        <AppIcon name="shield" :size="16" />
        {{ t('meetingRoom.takeOver') }}
      </AppButton>
    </div>

    <span class="divider" aria-hidden="true"></span>

    <div class="people">
      <div
        class="stack"
        role="group"
        :aria-label="
          t('meetingRoom.presence', { count: meeting.participants.length, online: onlineCount })
        "
      >
        <PresenceAvatar
          v-for="p in meeting.participants"
          :key="p.id"
          class="stack__item"
          :user-id="p.id"
          :name="p.displayName"
          :picture-version="p.pictureVersion"
          :online="p.online"
          :size="36"
        />
      </div>
      <AppButton
        v-if="store.isActive"
        size="touch"
        :aria-label="t('meetingRoom.inviteLabel')"
        @click="copyInvite"
      >
        <AppIcon :name="copied ? 'check' : 'userPlus'" :size="16" />
        {{ copied ? t('common.copied') : t('meetingRoom.invite') }}
      </AppButton>
      <span class="visually-hidden" role="status">{{ inviteStatus }}</span>
    </div>
  </section>
</template>

<style scoped>
.phase {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px 20px;
}
.phase__text {
  display: flex;
  flex: 1 1 300px;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.phase__of {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.phase__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.02em;
}
.phase__hint {
  margin: 0;
  color: var(--muted);
  font-size: 14px;
}
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}
.divider {
  width: 1px;
  height: 32px;
  background: var(--line);
}
.people {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 14px;
}
.stack {
  display: flex;
  align-items: center;
}
.stack__item + .stack__item {
  margin-left: -8px;
}
</style>
