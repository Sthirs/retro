<script setup lang="ts">
/** "4/6 ready" and who, for the facilitator (Reflect artboard; spec §8.2). */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

import UserAvatar from '@/components/UserAvatar.vue';
import { firstName } from '@/features/meetings/firstName';
import { useMeetingStore } from '@/stores/meeting';

const { t, locale } = useI18n();
const store = useMeetingStore();
const participants = computed(() => store.meeting?.participants ?? []);
const waiting = computed(() => {
  const names = participants.value.filter((p) => !p.ready).map((p) => firstName(p.displayName));
  if (names.length === 0) return t('ready.everyoneReady');
  const list = new Intl.ListFormat(locale.value, { type: 'conjunction' }).format(names);
  return t('ready.waitingFor', { names: list });
});
</script>

<template>
  <span class="ready" :title="waiting" data-testid="ready-count">
    <span class="ready__count">
      {{ t('ready.count', { ready: store.readyCount, total: participants.length }) }}
    </span>
    <span class="ready__label">{{ t('ready.label') }}</span>
    <span class="ready__avatars" role="img" :aria-label="waiting">
      <span
        v-for="p in participants"
        :key="p.id"
        class="ready__avatar"
        :class="{ 'ready__avatar--waiting': !p.ready }"
      >
        <UserAvatar
          :user-id="p.id"
          :name="p.displayName"
          :picture-version="p.pictureVersion"
          :size="24"
        />
      </span>
    </span>
  </span>
</template>

<style scoped>
.ready {
  display: flex;
  align-items: center;
  gap: 10px;
  white-space: nowrap;
}
.ready__count {
  color: var(--text);
  font-size: 18px;
  font-weight: 700;
}
.ready__label {
  color: var(--muted);
  font-size: 13px;
}
.ready__avatars {
  display: flex;
  align-items: center;
}
.ready__avatar + .ready__avatar {
  margin-left: -6px;
}
.ready__avatar--waiting {
  opacity: 0.4;
}
</style>
