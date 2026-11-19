<script setup lang="ts">
/**
 * The read-only summary of an ended meeting (spec §8.3, Summary artboard):
 * date, duration, facilitator and votes cast; the groups by votes with their
 * anonymous reflections; the tasks written in it with their current board
 * status; who took part. A meeting still running opens its room instead.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink, useRoute, useRouter } from 'vue-router';

import { api, apiError, errorCode, type EndedMeetingSummaryDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import RichText from '@/components/RichText.vue';
import TaskStatusPill from '@/components/TaskStatusPill.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useChannel } from '@/composables/useChannel';
import { useErrorText } from '@/composables/useErrorText';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import SummaryGroup from '@/features/meetings/summary/SummaryGroup.vue';
import { useDuration } from '@/features/meetings/useDuration';
import { formatDate, formatTime, isSameDay } from '@/lib/time';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const errorText = useErrorText();
const formatDuration = useDuration();

const teamId = computed(() => String(route.params.teamId));
const meetingId = computed(() => String(route.params.meetingId));
const summary = ref<EndedMeetingSummaryDto | null>(null);
/** The error code when the summary cannot be shown. */
const error = ref<string | null>(null);
let loadSeq = 0;

/** Loads the summary; `silent` keeps the shown one until the new one lands. */
async function load(id: string, silent = false): Promise<void> {
  const seq = ++loadSeq;
  if (!silent) {
    summary.value = null;
    error.value = null;
  }
  try {
    const res = await api.getMeetingSummary(id);
    if (res.status !== 200) throw apiError(res);
    if (seq !== loadSeq) return;
    // A link to another team's meeting is not this team's summary.
    if (res.data.teamId !== teamId.value) {
      error.value = 'meeting_not_found';
      return;
    }
    summary.value = res.data;
  } catch (e) {
    if (seq !== loadSeq) return;
    // A failed refresh keeps what is shown: it is still the meeting's summary.
    if (silent && summary.value) return;
    const code = errorCode(e);
    if (code === 'meeting_active') {
      await router.replace({
        name: 'meeting-room',
        params: { teamId: teamId.value, meetingId: id },
      });
      return;
    }
    error.value = code;
  }
}
watch(meetingId, (id) => void load(id), { immediate: true });

// Task statuses are the board's current ones (spec §8.3): follow the team's
// task events while the summary is open, and catch up after a reconnect.
const REFRESH_DELAY_MS = 300;
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
function refreshSoon(): void {
  if (refreshTimer !== undefined) return;
  refreshTimer = setTimeout(() => {
    refreshTimer = undefined;
    if (summary.value) void load(meetingId.value, true);
  }, REFRESH_DELAY_MS);
}
useChannel(
  () => (summary.value ? `team:${teamId.value}` : null),
  (event) => {
    if (event.type === 'resync' || event.type.startsWith('task.')) refreshSoon();
  },
);

// A response that lands after the user has left must not redirect them.
onBeforeUnmount(() => {
  loadSeq++;
  clearTimeout(refreshTimer);
});

/** "Ended at 18:07", with its date when it ended on a later day than it began. */
const endedText = computed(() => {
  const s = summary.value;
  if (!s) return '';
  const time = formatTime(s.endedAt);
  return isSameDay(s.endedAt, Date.parse(s.createdAt))
    ? t('summary.endedAt', { time })
    : t('summary.endedOn', { date: formatDate(s.endedAt), time });
});
const duration = computed(() =>
  summary.value ? formatDuration(summary.value.createdAt, summary.value.endedAt) : '',
);
const facts = computed(() => {
  const s = summary.value;
  if (!s) return [];
  const facts = [
    { key: 'date', label: t('summary.date'), value: formatDate(s.createdAt) },
    { key: 'duration', label: t('summary.duration'), value: duration.value },
    { key: 'facilitator', label: t('summary.facilitator'), value: s.facilitator.displayName },
  ];
  // Hidden when the meeting ended before Discuss: nobody saw the totals.
  if (s.votesCast != null) {
    facts.push({ key: 'votesCast', label: t('summary.votesCast'), value: String(s.votesCast) });
  }
  return facts;
});
const meetingsRoute = computed(() => ({ name: 'team-meetings', params: { teamId: teamId.value } }));
const boardRoute = computed(() => ({
  name: 'team-tasks',
  params: { teamId: teamId.value },
  query: { meeting: meetingId.value },
}));
</script>

<template>
  <div class="summary">
    <RouterLink :to="meetingsRoute" class="summary__back">
      <AppIcon name="arrowLeft" :size="16" />
      {{ t('summary.back') }}
    </RouterLink>

    <LinkStateCard
      v-if="error"
      class="summary__error"
      icon="alert"
      :title="t('summary.notFound')"
      :text="errorText(error)"
    >
      <AppButton :to="meetingsRoute">{{ t('meetingRoom.backToMeetings') }}</AppButton>
    </LinkStateCard>

    <p v-else-if="!summary" class="summary__loading" aria-busy="true">
      {{ t('summary.loading') }}
    </p>

    <template v-else>
      <div class="summary__header">
        <div class="summary__heading">
          <span class="summary__eyebrow">{{ t('summary.eyebrow') }}</span>
          <h1 class="summary__title">{{ summary.title }}</h1>
        </div>
        <span class="summary__ended" data-testid="ended-at">
          <AppIcon name="flag" :size="14" />
          {{ endedText }}
        </span>
      </div>

      <dl class="facts" :aria-label="t('summary.facts')">
        <div v-for="fact in facts" :key="fact.key" class="facts__item" :data-fact="fact.key">
          <dt class="facts__label">{{ fact.label }}</dt>
          <dd class="facts__value">{{ fact.value }}</dd>
        </div>
      </dl>

      <div class="summary__body">
        <section class="groups" aria-labelledby="summary-groups">
          <h2 id="summary-groups" class="section__title">{{ t('summary.groups') }}</h2>
          <SummaryGroup
            v-for="(group, index) in summary.groups"
            :key="group.id"
            :group="group"
            :rank="index + 1"
          />
          <p v-if="summary.groups.length === 0" class="empty">{{ t('summary.noGroups') }}</p>
        </section>

        <div class="summary__side">
          <section class="panel" aria-labelledby="summary-tasks">
            <div class="panel__header">
              <h2 id="summary-tasks" class="section__title">{{ t('summary.tasks') }}</h2>
              <RouterLink :to="boardRoute" class="panel__link">
                {{ t('summary.board') }}
                <AppIcon name="chevronRight" :size="14" />
              </RouterLink>
            </div>
            <p class="panel__hint">{{ t('summary.tasksHint') }}</p>
            <ul v-if="summary.tasks.length" class="tasks">
              <li
                v-for="task in summary.tasks"
                :key="task.id"
                class="task"
                data-testid="summary-task"
              >
                <div class="task__row">
                  <RichText :html="task.contentHtml" class="task__content" />
                  <TaskStatusPill :status="task.status" />
                </div>
                <ul
                  v-if="task.mentions.length"
                  class="task__mentions"
                  :aria-label="t('summary.mentions')"
                >
                  <li v-for="person in task.mentions" :key="person.id" class="task__mention">
                    @{{ person.displayName }}
                  </li>
                </ul>
              </li>
            </ul>
            <p v-else class="panel__empty">{{ t('summary.noTasks') }}</p>
          </section>

          <section class="panel" aria-labelledby="summary-participants">
            <h2 id="summary-participants" class="section__title">
              {{ t('summary.participants', { count: summary.participants.length }) }}
            </h2>
            <ul class="people">
              <li v-for="person in summary.participants" :key="person.id" class="people__item">
                <UserAvatar
                  :user-id="person.id"
                  :name="person.displayName"
                  :picture-version="person.pictureVersion"
                  :size="28"
                  ring="var(--surface)"
                />
                <span class="people__name">{{ person.displayName }}</span>
              </li>
            </ul>
          </section>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.summary {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 100%;
  min-width: 0;
}
.summary__back {
  display: inline-flex;
  align-items: center;
  align-self: flex-start;
  gap: 6px;
  min-height: 44px;
  color: var(--accent-text);
  font-size: 14px;
  font-weight: 600;
  text-decoration: none;
}
.summary__back:hover {
  color: var(--text);
}
.summary__error {
  align-self: flex-start;
}
.summary__loading {
  margin: 0;
  color: var(--muted);
}
.summary__header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
}
.summary__heading {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}
.summary__eyebrow {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.summary__title {
  margin: 0;
  font-size: 30px;
  font-weight: 700;
  letter-spacing: -0.01em;
  overflow-wrap: anywhere;
}
.summary__ended {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  background: var(--tag-neutral);
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 20px 48px;
  margin: 0;
  padding: 18px 24px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.facts__item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.facts__label {
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}
.facts__value {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
}
.summary__body {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: 20px;
}
.groups {
  display: flex;
  flex: 999 1 520px;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.summary__side {
  display: flex;
  flex: 1 1 320px;
  flex-direction: column;
  gap: 20px;
  min-width: 0;
}
.section__title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.empty {
  margin: 0;
  padding: 20px;
  border: 1px dashed var(--line);
  border-radius: var(--radius-panel);
  color: var(--muted);
  font-size: 14px;
}
.panel {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 18px 20px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.panel__link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-height: 44px;
  color: var(--accent-text);
  font-size: 13px;
  font-weight: 600;
  text-decoration: none;
}
.panel__link:hover {
  color: var(--text);
}
.panel__hint,
.panel__empty {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.5;
}
.panel__empty {
  padding-top: 8px;
}
.tasks {
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
}
.task {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
}
.task__row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}
.task__content {
  min-width: 0;
  font-size: 14px;
  line-height: 1.45;
  overflow-wrap: anywhere;
}
.task__mentions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 13px;
}
.task__mention {
  display: inline-flex;
  padding: 0 6px;
  border-radius: var(--radius-xs);
  background: var(--accent-soft);
  color: var(--accent-text);
  font-weight: 600;
}
.people {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
}
.people__item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  font-size: 14px;
}
.people__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
