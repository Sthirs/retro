<script setup lang="ts">
/** Top bar (spec §7.1): logo, team selector, pending invitations and user menu. */
import { useI18n } from 'vue-i18n';
import { RouterLink } from 'vue-router';

import TeamSelector from '@/components/layout/TeamSelector.vue';
import UserMenu from '@/components/layout/UserMenu.vue';
import RetroLogo from '@/components/RetroLogo.vue';

withDefaults(defineProps<{ teamSelector?: boolean }>(), { teamSelector: false });
const { t } = useI18n();
</script>

<template>
  <header class="topbar">
    <RouterLink to="/" class="brand" :aria-label="t('topBar.home')">
      <RetroLogo :size="32" />
      <span class="brand__wordmark" aria-hidden="true">retro</span>
    </RouterLink>
    <template v-if="teamSelector">
      <span class="topbar__divider" aria-hidden="true" />
      <TeamSelector />
    </template>
    <div class="topbar__spacer" />
    <UserMenu />
  </header>
</template>

<style scoped>
.topbar {
  position: relative;
  z-index: 10;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 16px;
  padding: 10px 24px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  text-decoration: none;
}
.brand__wordmark {
  color: var(--text);
  font-size: 22px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.topbar__divider {
  width: 1px;
  height: 24px;
  background: var(--line);
}
.topbar__spacer {
  flex: 1 1 0;
}
@media (max-width: 560px) {
  .topbar {
    gap: 8px;
    padding: 8px 16px;
  }
  .brand__wordmark,
  .topbar__divider {
    display: none;
  }
}
</style>
