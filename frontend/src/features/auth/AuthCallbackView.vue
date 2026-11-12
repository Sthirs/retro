<script setup lang="ts">
import { onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import RetroLogo from '@/components/RetroLogo.vue';
import { safeRedirect } from '@/lib/redirect';
import { useSessionStore } from '@/stores/session';

/**
 * Landing page after single sign-on. The backend has set the refresh cookie;
 * exchange it for an access token, then continue. No token is in the URL.
 */
const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();

onMounted(async () => {
  const target = safeRedirect(route.query.redirect);
  if (session.isAuthenticated || (await session.refresh())) {
    await router.replace(target);
  } else {
    await router.replace({ name: 'login', query: { error: 'sso_failed' } });
  }
});
</script>

<template>
  <main class="callback" aria-busy="true">
    <RetroLogo :size="48" />
    <p>{{ t('callback.signingIn') }}</p>
  </main>
</template>

<style scoped>
.callback {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--muted);
}
</style>
