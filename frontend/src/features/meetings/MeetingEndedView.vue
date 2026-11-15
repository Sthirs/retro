<script setup lang="ts">
/**
 * An ended meeting, read-only. A stand-in until the meeting summary (spec
 * §8.3, Summary artboard) is built: what it was, who ran it, who was there.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { MeetingStateDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import { useDuration } from '@/features/meetings/useDuration';
import { formatDate } from '@/lib/time';

const props = defineProps<{ meeting: MeetingStateDto }>();
const { t } = useI18n();
const formatDuration = useDuration();

const facilitator = computed(
  () => props.meeting.participants.find((p) => p.id === props.meeting.facilitatorId) ?? null,
);
const text = computed(() =>
  t('meetingEnded.text', {
    date: formatDate(props.meeting.createdAt),
    duration: props.meeting.endedAt
      ? formatDuration(props.meeting.createdAt, props.meeting.endedAt)
      : '',
    name: facilitator.value?.displayName ?? '',
  }),
);
</script>

<template>
  <LinkLayout>
    <section class="ended">
      <span class="ended__icon" aria-hidden="true"><AppIcon name="flag" :size="26" /></span>
      <div class="ended__text">
        <span class="ended__label">{{ t('meetingEnded.title') }}</span>
        <h1 class="ended__title">{{ meeting.title }}</h1>
        <p class="ended__body">{{ text }}</p>
      </div>
      <div class="ended__people">
        <span class="stack">
          <UserAvatar
            v-for="p in meeting.participants"
            :key="p.id"
            class="stack__item"
            :user-id="p.id"
            :name="p.displayName"
            :picture-version="p.pictureVersion"
            :size="28"
          />
        </span>
        {{
          t(
            'meetingEnded.participants',
            { count: meeting.participants.length },
            meeting.participants.length,
          )
        }}
      </div>
      <AppButton :to="{ name: 'team-meetings', params: { teamId: meeting.teamId } }">
        {{ t('meetingRoom.backToMeetings') }}
      </AppButton>
    </section>
  </LinkLayout>
</template>

<style scoped>
.ended {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 16px;
  width: 100%;
  max-width: 480px;
  box-sizing: border-box;
  padding: 28px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.ended__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--col-delta-bg);
  color: var(--col-delta-fg);
}
.ended__text {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ended__label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.ended__title {
  margin: 0;
  font-size: 20px;
  font-weight: 700;
}
.ended__body {
  margin: 0;
  color: var(--muted);
}
.ended__people {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--muted);
  font-size: 13px;
}
.stack {
  display: flex;
}
.stack__item + .stack__item {
  margin-left: -6px;
}
</style>
