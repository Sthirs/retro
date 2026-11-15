<script setup lang="ts">
/** An ended meeting in the Meetings page's "Ended" list (Meetings artboard). */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink } from 'vue-router';

import type { MeetingSummaryDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { useDuration } from '@/features/meetings/useDuration';
import { formatDate } from '@/lib/time';

const props = defineProps<{ meeting: MeetingSummaryDto }>();
const { t } = useI18n();
const formatDuration = useDuration();

const duration = computed(() =>
  props.meeting.endedAt ? formatDuration(props.meeting.createdAt, props.meeting.endedAt) : '',
);
</script>

<template>
  <li>
    <RouterLink
      class="row"
      :to="{ name: 'meeting-room', params: { teamId: meeting.teamId, meetingId: meeting.id } }"
    >
      <span class="row__title">{{ meeting.title }}</span>
      <span class="row__muted row__date">{{ formatDate(meeting.createdAt) }}</span>
      <span class="row__muted row__duration">{{ duration }}</span>
      <span class="row__muted row__facilitator">
        <UserAvatar
          :user-id="meeting.facilitator.id"
          :name="meeting.facilitator.displayName"
          :picture-version="meeting.facilitator.pictureVersion"
          :size="22"
        />
        {{ meeting.facilitator.displayName }}
      </span>
      <span class="row__muted row__counts">
        {{ t('meetings.groups', { count: meeting.groupCount }, meeting.groupCount) }} ·
        {{ t('meetings.tasks', { count: meeting.taskCount }, meeting.taskCount) }}
      </span>
      <span class="row__summary">
        {{ t('meetings.summary') }}
        <AppIcon name="chevronRight" :size="16" />
      </span>
    </RouterLink>
  </li>
</template>

<style scoped>
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 24px;
  min-height: 56px;
  box-sizing: border-box;
  padding: 10px 20px;
  border-top: 1px solid var(--line);
  color: var(--text);
  text-decoration: none;
}
.row:hover {
  background: var(--well);
}
.row__title {
  flex: 1 1 180px;
  font-weight: 700;
}
.row__muted {
  color: var(--muted);
  font-size: 14px;
}
.row__date {
  flex: 1 1 120px;
}
.row__duration {
  flex: 0 1 100px;
}
.row__facilitator {
  display: flex;
  flex: 1 1 160px;
  align-items: center;
  gap: 8px;
}
.row__counts {
  flex: 1 1 140px;
}
.row__summary {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--accent-text);
  font-size: 14px;
  font-weight: 600;
}
</style>
