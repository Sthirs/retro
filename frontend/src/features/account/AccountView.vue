<script setup lang="ts">
/** Account page: the signed-in user's own settings (spec §7.2). */
import { useI18n } from 'vue-i18n';

const { t } = useI18n();
</script>

<template>
  <div class="page">
    <h1 class="page__title">{{ t('account.title') }}</h1>
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
.page__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
</style>
