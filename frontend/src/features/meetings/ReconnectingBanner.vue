<script setup lang="ts">
/** Shown while the real-time socket is down (Reconnecting artboard, spec §15.4). */
import { useI18n } from 'vue-i18n';

import AppIcon from '@/components/AppIcon.vue';

const { t } = useI18n();
</script>

<template>
  <div class="banner" role="status">
    <AppIcon name="wifiOff" :size="18" />
    {{ t('meetingRoom.reconnecting') }}
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 10px 24px;
  background: var(--notice-bg);
  color: var(--notice-text);
  font-size: 14px;
  font-weight: 600;
  text-align: center;
}
</style>
