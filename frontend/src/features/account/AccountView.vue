<script setup lang="ts">
/** Account page: the signed-in user's own settings (spec §7.2). */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { api, apiError, type SignInMethod } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AccountPanel from '@/features/account/AccountPanel.vue';
import InvitationsSection from '@/features/account/InvitationsSection.vue';
import PreferencesSection from '@/features/account/PreferencesSection.vue';
import ProfileSection from '@/features/account/ProfileSection.vue';
import TeamsSection from '@/features/account/TeamsSection.vue';
import { useSessionStore } from '@/stores/session';

const { t } = useI18n();
const router = useRouter();
const session = useSessionStore();

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

async function logout(): Promise<void> {
  try {
    await session.logout();
  } catch {
    // The session is cleared locally even when the request fails.
  }
  await router.replace({ name: 'login' });
}
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
    <TeamsSection />
    <InvitationsSection />
    <PreferencesSection />
    <section class="session" :aria-label="t('account.session.label')">
      <div class="session__text">
        <h2 class="session__title">{{ t('account.session.title') }}</h2>
        <p class="session__description">{{ t('account.session.text') }}</p>
      </div>
      <AppButton size="touch" @click="logout">
        <AppIcon name="logout" :size="16" />
        {{ t('account.session.title') }}
      </AppButton>
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
.session {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.session__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.session__title {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--text);
}
.session__description {
  margin: 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--muted);
}
</style>
