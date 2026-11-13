<script setup lang="ts">
/** Team pages: top bar with the team selector, left menu, content (spec §7). */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink, RouterView, useRoute } from 'vue-router';

import AppIcon from '@/components/AppIcon.vue';
import type { IconName } from '@/components/icons';
import AppTopBar from '@/components/layout/AppTopBar.vue';

const { t } = useI18n();
const route = useRoute();
const teamId = computed(() => String(route.params.teamId));

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
      <nav class="nav" :aria-label="t('nav.label')">
        <RouterLink
          v-for="entry in entries"
          :key="entry.name"
          :to="{ name: entry.name, params: { teamId } }"
          class="nav__link"
          :aria-current="route.name === entry.name ? 'page' : undefined"
        >
          <AppIcon :name="entry.icon" />
          {{ t(entry.label) }}
        </RouterLink>
      </nav>
      <main class="shell__main">
        <RouterView :key="teamId" />
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
