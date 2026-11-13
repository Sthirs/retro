<script setup lang="ts">
/** Placeholder for the Meetings and Tasks sections until they are built. */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { RouterLink, useRoute } from 'vue-router';

import AppIcon from '@/components/AppIcon.vue';
import { useTeamsStore } from '@/stores/teams';

const props = defineProps<{ section: 'meetings' | 'tasks' }>();
const { t } = useI18n();
const route = useRoute();
const teams = useTeamsStore();
const team = computed(() => teams.byId(String(route.params.teamId)));
</script>

<template>
  <div v-if="team" class="page">
    <div class="page__header">
      <h1 class="page__title">{{ t(`comingSoon.${props.section}`) }}</h1>
      <p class="page__subtitle">
        {{
          t('comingSoon.subtitle', { team: team.name, count: team.memberCount }, team.memberCount)
        }}
      </p>
    </div>
    <section class="empty">
      <span class="empty__icon" aria-hidden="true">
        <AppIcon :name="section === 'meetings' ? 'calendar' : 'tasks'" :size="26" />
      </span>
      <p class="empty__text">{{ t('comingSoon.text') }}</p>
      <RouterLink :to="{ name: 'team-settings', params: { teamId: team.id } }" class="empty__link">
        {{ t('comingSoon.toSettings') }}
        <AppIcon name="arrowRight" :size="16" />
      </RouterLink>
    </section>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: 20px;
  width: 100%;
  max-width: 920px;
}
.page__header {
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
}
.empty {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 14px;
  padding: 28px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.empty__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--accent-soft);
  color: var(--accent-text);
}
.empty__text {
  margin: 0;
  color: var(--muted);
}
.empty__link {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  font-weight: 600;
}
</style>
