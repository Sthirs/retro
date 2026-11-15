<script setup lang="ts">
/** An active meeting on the Meetings page (Meetings artboard). */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import type { MeetingSummaryDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import PresenceAvatar from '@/components/PresenceAvatar.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { formatDate, formatTime, isSameDay } from '@/lib/time';

const props = defineProps<{ meeting: MeetingSummaryDto; meId: string | null }>();
const { t } = useI18n();

const started = computed(() => {
  const time = formatTime(props.meeting.createdAt);
  return isSameDay(props.meeting.createdAt)
    ? t('meetings.startedToday', { time })
    : t('meetings.startedOn', { date: formatDate(props.meeting.createdAt), time });
});
const joined = computed(() => props.meeting.participants.some((p) => p.id === props.meId));
const action = computed(() => (joined.value ? 'rejoin' : 'join'));
</script>

<template>
  <article class="card">
    <div class="card__top">
      <div class="card__heading">
        <h3 class="card__title">{{ meeting.title }}</h3>
        <span class="card__started">{{ started }}</span>
      </div>
      <span class="live"
        ><span class="live__dot" aria-hidden="true"></span>{{ t('meetings.live') }}</span
      >
    </div>
    <div class="card__meta">
      <span class="card__facilitator">
        <UserAvatar
          :user-id="meeting.facilitator.id"
          :name="meeting.facilitator.displayName"
          :picture-version="meeting.facilitator.pictureVersion"
          :size="22"
        />
        {{ t('meetings.facilitator') }}
        <strong>{{ meeting.facilitator.displayName }}</strong>
      </span>
      <span>
        {{ t('meetings.phase') }} <strong>{{ t(`phases.${meeting.phase}`) }}</strong>
      </span>
    </div>
    <div class="card__footer">
      <span class="card__people">
        <span class="stack">
          <PresenceAvatar
            v-for="p in meeting.participants"
            :key="p.id"
            class="stack__item"
            :user-id="p.id"
            :name="p.displayName"
            :picture-version="p.pictureVersion"
            :online="p.online"
            :size="28"
          />
        </span>
        <span class="card__count">
          {{
            t(
              'meetings.participants',
              { count: meeting.participants.length },
              meeting.participants.length,
            )
          }}
        </span>
      </span>
      <AppButton
        variant="primary"
        :to="{ name: 'meeting-room', params: { teamId: meeting.teamId, meetingId: meeting.id } }"
        :aria-label="t(`meetings.${action}Label`, { title: meeting.title })"
      >
        {{ t(`meetings.${action}`) }}
        <AppIcon name="arrowRight" :size="16" />
      </AppButton>
    </div>
  </article>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.card__top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.card__heading {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.card__title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.card__started {
  color: var(--muted);
  font-size: 13px;
}
.live {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--tag-neutral);
  color: var(--text);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.live__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--success);
}
.card__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 20px;
  color: var(--muted);
  font-size: 13px;
}
.card__meta strong {
  color: var(--text);
}
.card__facilitator {
  display: flex;
  align-items: center;
  gap: 8px;
}
.card__footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--line);
}
.card__people {
  display: flex;
  align-items: center;
  gap: 10px;
}
.stack {
  display: flex;
  align-items: center;
}
.stack__item + .stack__item {
  margin-left: -6px;
}
.card__count {
  color: var(--muted);
  font-size: 13px;
}
</style>
