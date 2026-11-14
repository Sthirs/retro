<script setup lang="ts">
/** Account page: the signed-in user's own settings (spec §7.2). */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, type SignInMethod } from '@/api/client';
import AppAlert from '@/components/ui/AppAlert.vue';
import AccountPanel from '@/features/account/AccountPanel.vue';
import ProfileSection from '@/features/account/ProfileSection.vue';

const { t } = useI18n();

/** What `GET /me` adds to the user: sign-in method and account age. */
const details = ref<{ memberSince: string; signInMethod: SignInMethod } | null>(null);
const failed = ref(false);

onMounted(async () => {
  try {
    const res = await api.me();
    if (res.status !== 200) throw apiError(res);
    details.value = { memberSince: res.data.memberSince, signInMethod: res.data.signInMethod };
  } catch {
    failed.value = true;
  }
});
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">{{ t('account.title') }}</h1>
      <p class="page__subtitle">{{ t('account.subtitle') }}</p>
    </header>
    <ProfileSection
      v-if="details"
      :member-since="details.memberSince"
      :sign-in-method="details.signInMethod"
    />
    <AccountPanel v-else id="profile" :title="t('account.profile.title')">
      <div class="page__state">
        <AppAlert v-if="failed">{{ t('account.loadFailed') }}</AppAlert>
        <p v-else class="page__loading">{{ t('common.loading') }}</p>
      </div>
    </AccountPanel>
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
  font-size: 14px;
  line-height: 1.5;
  color: var(--muted);
}
.page__state {
  padding: 0 20px 22px;
}
.page__loading {
  margin: 0;
  font-size: 14px;
  color: var(--muted);
}
</style>
