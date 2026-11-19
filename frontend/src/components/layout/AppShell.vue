<script setup lang="ts">
/**
 * Team pages: top bar with the team selector, left menu, content (spec §7).
 * Pages outside a team (the account page) keep the shell around the user's
 * remembered team.
 */
import { computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router';

import AppIcon from '@/components/AppIcon.vue';
import type { IconName } from '@/components/icons';
import AppTopBar from '@/components/layout/AppTopBar.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import { useChannel } from '@/composables/useChannel';
import { useErrorText } from '@/composables/useErrorText';
import { useStartMeeting } from '@/composables/useStartMeeting';
import { useMeetingsStore } from '@/stores/meetings';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const teams = useTeamsStore();
const teamId = computed(() => {
  const param = route.params.teamId;
  return typeof param === 'string' ? param : teams.currentTeam()?.id;
});
// Team pages remount when the team changes. Other pages (the account page)
// must stay mounted when the remembered or default team changes, so they key
// on the route, not on the resolved team.
const viewKey = computed(() =>
  typeof route.params.teamId === 'string' ? route.params.teamId : String(route.name),
);

const meetings = useMeetingsStore();
const starter = useStartMeeting();
const errorText = useErrorText();

// The team's meetings feed the Meetings badge and page; teammates' changes
// arrive on the team channel (spec §11.1).
watch(
  teamId,
  (id) => {
    if (id) void meetings.load(id);
  },
  { immediate: true },
);
/**
 * The server ended the team channel: the user was removed, left in another
 * tab, or the team was deleted (spec §11.1). Their teams are reloaded, and a
 * page of that team goes home, which opens another team or onboarding.
 */
async function leaveTeam(id: string): Promise<void> {
  await teams.load().catch(() => undefined);
  if (route.params.teamId === id) await router.replace({ name: 'home' });
}
useChannel(
  () => (teamId.value ? `team:${teamId.value}` : null),
  (event) => {
    if (event.type === 'error') {
      void leaveTeam(event.channel.slice('team:'.length));
      return;
    }
    if (event.type === 'presence.changed') {
      const { meetingId, userId, online } = event.payload;
      meetings.applyPresence(meetingId as string, userId as string, online === true);
      return;
    }
    if (event.type.startsWith('meeting.') || event.type === 'resync') meetings.reloadSoon();
  },
);
const activeCount = computed(() => (meetings.teamId === teamId.value ? meetings.active.length : 0));

/** A meeting's summary belongs to the Meetings section. */
function isCurrent(name: string): boolean {
  return route.name === name || (name === 'team-meetings' && route.name === 'meeting-summary');
}

const entries: { name: string; label: string; icon: IconName }[] = [
  { name: 'team-meetings', label: 'nav.meetings', icon: 'calendar' },
  { name: 'team-tasks', label: 'nav.tasks', icon: 'tasks' },
  { name: 'team-settings', label: 'nav.settings', icon: 'settings' },
];
</script>

<template>
  <div class="shell">
    <AppTopBar team-selector />
    <div class="shell__body">
      <nav v-if="teamId" class="nav" :aria-label="t('nav.label')">
        <AppButton
          class="nav__add"
          variant="primary"
          size="touch"
          block
          :disabled="starter.busy.value"
          @click="starter.start(teamId)"
        >
          <AppIcon name="plus" :size="16" />
          {{ t('meetings.add') }}
        </AppButton>
        <AppAlert v-if="starter.error.value">{{ errorText(starter.error.value) }}</AppAlert>
        <RouterLink
          v-for="entry in entries"
          :key="entry.name"
          :to="{ name: entry.name, params: { teamId } }"
          class="nav__link"
          :aria-current="isCurrent(entry.name) ? 'page' : undefined"
        >
          <AppIcon :name="entry.icon" />
          {{ t(entry.label) }}
          <span
            v-if="entry.name === 'team-meetings' && activeCount > 0"
            class="nav__badge"
            :aria-label="t('meetings.activeCount', { count: activeCount }, activeCount)"
            >{{ activeCount }}</span
          >
        </RouterLink>
      </nav>
      <main class="shell__main">
        <RouterView :key="viewKey" />
      </main>
    </div>
  </div>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
.shell__body {
  display: flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  align-items: stretch;
}
.nav {
  display: flex;
  flex: 1 1 220px;
  flex-direction: column;
  gap: 6px;
  max-width: 100%;
  padding: 20px 16px;
  background: var(--surface);
  border-right: 1px solid var(--line);
}
.nav__link {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 0 12px;
  border-radius: var(--radius-control);
  color: var(--muted);
  font-size: 14px;
  font-weight: 600;
  text-decoration: none;
}
.nav__add {
  margin-bottom: 14px;
}
.nav__badge {
  margin-left: auto;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 700;
}
.nav__link:hover {
  background: var(--well);
  color: var(--text);
}
.nav__link[aria-current='page'] {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.shell__main {
  display: flex;
  flex: 999 1 560px;
  flex-direction: column;
  gap: 24px;
  min-width: 0;
  padding: 28px 32px;
}
@media (max-width: 800px) {
  .nav__add {
    width: auto;
    margin-bottom: 0;
  }
  .nav {
    flex-direction: row;
    flex-wrap: wrap;
    padding: 8px 16px;
    border-right: none;
    border-bottom: 1px solid var(--line);
  }
  .shell__main {
    padding: 20px 16px;
  }
}
</style>
