<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import RetroLogo from '@/components/RetroLogo.vue';
import AppButton from '@/components/ui/AppButton.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import { useSessionStore } from '@/stores/session';

/** Placeholder home until onboarding and teams land (wave 2). */
const { t } = useI18n();
const router = useRouter();
const session = useSessionStore();

async function signOut(): Promise<void> {
  await session.logout();
  await router.replace({ name: 'login' });
}
</script>

<template>
  <div v-if="session.user" class="shell">
    <header class="topbar">
      <div class="brand">
        <RetroLogo :size="32" />
        <span class="brand__wordmark">retro</span>
      </div>
      <div class="account">
        <UserAvatar :user-id="session.user.id" :name="session.user.displayName" />
        <span class="account__name" data-testid="current-user">{{ session.user.displayName }}</span>
        <AppButton variant="secondary" @click="signOut">{{ t('home.signOut') }}</AppButton>
      </div>
    </header>
    <main class="content">
      <section class="panel">
        <h1 class="panel__title">{{ t('home.welcome', { name: session.user.displayName }) }}</h1>
        <p>{{ t('home.signedInAs', { email: session.user.email }) }}</p>
        <p class="panel__muted">{{ t('home.comingSoon') }}</p>
      </section>
    </main>
  </div>
</template>

<style scoped>
.shell {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-wrap: wrap;
  padding: 12px 24px;
  background: var(--surface);
  border-bottom: 1px solid var(--line);
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
}
.brand__wordmark {
  font-size: 22px;
  font-weight: 800;
  letter-spacing: -0.03em;
}
.account {
  display: flex;
  align-items: center;
  gap: 12px;
}
.account__name {
  font-weight: 600;
}
.content {
  flex: 1 1 auto;
  padding: 32px 24px;
  display: flex;
  justify-content: center;
}
.panel {
  width: 100%;
  max-width: 720px;
  padding: 28px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.panel__title {
  margin: 0 0 8px;
  font-size: 28px;
  font-weight: 700;
}
.panel p {
  margin: 0 0 4px;
}
.panel__muted {
  color: var(--muted);
}
</style>
