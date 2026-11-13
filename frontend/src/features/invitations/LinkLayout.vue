<script setup lang="ts">
/** Bare page for invitation and join links: logo header, centered content. */
import { useI18n } from 'vue-i18n';
import { RouterLink } from 'vue-router';

import RetroLogo from '@/components/RetroLogo.vue';

const { t } = useI18n();
</script>

<template>
  <div class="page">
    <header class="page__header">
      <RouterLink to="/" class="brand" :aria-label="t('topBar.home')">
        <RetroLogo :size="32" />
        <span class="brand__wordmark" aria-hidden="true">retro</span>
      </RouterLink>
    </header>
    <main class="page__main">
      <slot />
    </main>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
.page__header {
  padding: 16px 24px;
}
.brand {
  display: inline-flex;
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
.page__main {
  display: flex;
  flex: 1 1 auto;
  align-items: center;
  justify-content: center;
  padding: 24px;
}
@media (max-width: 560px) {
  .page__main {
    align-items: flex-start;
    padding: 16px;
  }
}
</style>
