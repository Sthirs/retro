<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import type { AuthConfig } from '@/api/client';
import { fetchAuthConfig, ssoUrl } from '@/auth/config';
import RetroLogo from '@/components/RetroLogo.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppCheckbox from '@/components/ui/AppCheckbox.vue';
import PasswordField from '@/components/ui/PasswordField.vue';
import TextField from '@/components/ui/TextField.vue';
import { safeRedirect } from '@/lib/redirect';
import { LoginError, useSessionStore } from '@/stores/session';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();

const SSO_ERRORS = ['sso_failed', 'sso_unavailable', 'sso_not_registered'] as const;
type ErrorKey =
  'invalid_credentials' | 'too_many_requests' | 'unknown' | 'missing' | (typeof SSO_ERRORS)[number];

const config = ref<AuthConfig | null>(null);
const login = ref('');
const password = ref('');
const remember = ref(false);
const submitting = ref(false);
const error = ref<ErrorKey | null>(SSO_ERRORS.find((code) => code === route.query.error) ?? null);

const redirect = computed(() => safeRedirect(route.query.redirect));
const ssoHref = computed(() => ssoUrl(redirect.value, remember.value));
const credentialsInvalid = computed(() => error.value === 'invalid_credentials');

onMounted(async () => {
  config.value = await fetchAuthConfig();
});

async function submit(): Promise<void> {
  if (submitting.value) return;
  if (!login.value.trim() || !password.value) {
    error.value = 'missing';
    return;
  }
  submitting.value = true;
  error.value = null;
  try {
    await session.login(login.value, password.value, remember.value);
    await router.replace(redirect.value);
  } catch (e) {
    error.value = e instanceof LoginError ? e.code : 'unknown';
    password.value = '';
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="page">
    <main class="page__main">
      <div class="card">
        <RetroLogo :size="56" />
        <div class="heading">
          <h1 class="heading__title">{{ t('signIn.title') }}</h1>
          <p class="heading__subtitle">{{ t('signIn.subtitle') }}</p>
        </div>

        <AppAlert v-if="error" data-testid="sign-in-error">
          {{ t(`signIn.errors.${error}`) }}
        </AppAlert>

        <template v-if="config">
          <form
            v-if="config.localLogin"
            class="form"
            novalidate
            aria-label="Sign in"
            @submit.prevent="submit"
          >
            <TextField
              id="si-user"
              v-model="login"
              :label="t('signIn.login')"
              type="text"
              name="username"
              autocomplete="username"
              autocapitalize="none"
              spellcheck="false"
              :placeholder="t('signIn.loginPlaceholder')"
              :aria-invalid="credentialsInvalid"
            />
            <PasswordField
              id="si-pass"
              v-model="password"
              :label="t('signIn.password')"
              :show-label="t('signIn.showPassword')"
              :hide-label="t('signIn.hidePassword')"
              name="password"
              autocomplete="current-password"
              :placeholder="t('signIn.passwordPlaceholder')"
              :aria-invalid="credentialsInvalid"
            >
              <template #aside>
                <a
                  v-if="config.passwordResetUrl"
                  :href="config.passwordResetUrl"
                  class="forgot"
                  rel="noopener"
                  >{{ t('signIn.forgotPassword') }}</a
                >
              </template>
            </PasswordField>
            <AppCheckbox v-model="remember" :label="t('signIn.remember')" />
            <AppButton type="submit" variant="primary" size="page" block :disabled="submitting">
              {{ submitting ? t('signIn.submitting') : t('signIn.submit') }}
            </AppButton>
          </form>

          <template v-if="config.sso.enabled">
            <div v-if="config.localLogin" class="divider" aria-hidden="true">
              <span class="divider__line" />{{ t('signIn.or') }}<span class="divider__line" />
            </div>
            <AppCheckbox
              v-if="!config.localLogin"
              v-model="remember"
              :label="t('signIn.remember')"
              class="remember-sso"
            />
            <AppButton :href="ssoHref" variant="secondary" size="page" block>
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <circle cx="7.5" cy="15.5" r="4.5" />
                <path d="m10.7 12.3 9.3-9.3M17 6l3 3" />
              </svg>
              {{ config.sso.buttonText || t('signIn.sso') }}
            </AppButton>
            <p class="hint">{{ t('signIn.ssoHint') }}</p>
          </template>
        </template>
      </div>
    </main>
  </div>
</template>

<style scoped>
.page {
  position: relative;
  min-height: 100vh;
  min-height: 100dvh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  color: var(--text);
}
.page__main {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}
.card {
  width: 100%;
  max-width: 420px;
  padding: 36px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 20px;
  text-align: center;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--radius-panel);
  box-shadow: var(--shadow-panel);
}
.heading {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.heading__title {
  margin: 0;
  font-size: 26px;
  font-weight: 700;
  letter-spacing: -0.01em;
  line-height: 1.25;
  color: var(--text);
}
.heading__subtitle {
  margin: 0;
  font-size: 14px;
  line-height: 1.5;
  color: var(--muted);
}
.form {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  text-align: left;
}
.forgot {
  font-size: 13px;
  font-weight: 600;
  text-decoration: none;
  color: var(--accent-text);
}
.forgot:hover {
  color: var(--text);
}
.divider {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: var(--muted);
}
.divider__line {
  flex: 1 1 auto;
  height: 1px;
  background: var(--line);
}
.remember-sso {
  align-self: flex-start;
}
.hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--muted);
}
@media (max-width: 480px) {
  .card {
    padding: 28px 20px;
  }
}
</style>
