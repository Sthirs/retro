<script setup lang="ts">
/**
 * The team's meetings (spec §7.2, Meetings artboard): the active ones as
 * cards with Join, then the ended ones, each opening its summary. The list
 * follows the team channel, so a meeting started by a teammate shows up live
 * (the shell keeps the store loaded and subscribed).
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute } from 'vue-router';

import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useErrorText } from '@/composables/useErrorText';
import { useStartMeeting } from '@/composables/useStartMeeting';
import EndedMeetingRow from '@/features/meetings/EndedMeetingRow.vue';
import MeetingCard from '@/features/meetings/MeetingCard.vue';
import { useMeetingsStore } from '@/stores/meetings';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const errorText = useErrorText();
const route = useRoute();
const teams = useTeamsStore();
const session = useSessionStore();
const store = useMeetingsStore();
const starter = useStartMeeting();

const teamId = computed(() => String(route.params.teamId));
const team = computed(() => teams.byId(teamId.value));
const ready = computed(() => store.loaded && store.teamId === teamId.value);
const error = computed(() => starter.error.value ?? (ready.value ? null : store.error));
</script>

<template>
  <div v-if="team" class="page">
    <div class="page__header">
      <div class="page__heading">
        <h1 class="page__title">{{ t('meetings.title') }}</h1>
        <p class="page__subtitle">
          {{
            t('meetings.subtitle', { team: team.name, count: team.memberCount }, team.memberCount)
          }}
        </p>
      </div>
      <AppButton
        variant="primary"
        size="touch"
        :disabled="starter.busy.value"
        @click="starter.start(teamId)"
      >
        <AppIcon name="plus" :size="16" />
        {{ t('meetings.add') }}
      </AppButton>
    </div>

    <AppAlert v-if="error">{{ errorText(error) }}</AppAlert>

    <p v-if="!ready && !error" class="page__loading" aria-busy="true">{{ t('common.loading') }}</p>
    <template v-else-if="ready">
      <section class="active" aria-labelledby="meetings-active">
        <h2 id="meetings-active" class="section__title">{{ t('meetings.active') }}</h2>
        <div v-if="store.active.length" class="active__grid">
          <MeetingCard
            v-for="meeting in store.active"
            :key="meeting.id"
            :meeting="meeting"
            :me-id="session.user?.id ?? null"
          />
        </div>
        <p v-else class="empty">{{ t('meetings.noActive') }}</p>
      </section>

      <section class="ended" aria-labelledby="meetings-ended">
        <div class="ended__header">
          <h2 id="meetings-ended" class="section__title">{{ t('meetings.ended') }}</h2>
        </div>
        <ul v-if="store.ended.length" class="ended__list">
          <EndedMeetingRow v-for="meeting in store.ended" :key="meeting.id" :meeting="meeting" />
        </ul>
        <p v-else class="ended__empty">{{ t('meetings.noEnded') }}</p>
      </section>
    </template>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 100%;
}
.page__header {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: 16px;
}
.page__heading {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.page__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.page__subtitle {
  margin: 0;
  color: var(--muted);
  font-size: 14px;
  line-height: 1.5;
}
.page__loading {
  margin: 0;
  color: var(--muted);
}
.section__title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.active {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.active__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(340px, 100%), 1fr));
  gap: 16px;
}
.empty {
  margin: 0;
  padding: 20px;
  border: 1px dashed var(--line);
  border-radius: var(--radius-panel);
  color: var(--muted);
  font-size: 14px;
}
.ended {
  overflow: hidden;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.ended__header {
  padding: 16px 20px;
}
.ended__list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.ended__empty {
  margin: 0;
  padding: 16px 20px;
  border-top: 1px solid var(--line);
  color: var(--muted);
  font-size: 14px;
}
</style>
