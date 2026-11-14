<script setup lang="ts">
/** Profile panel: picture, display name, read-only email and account facts. */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, errorCode, SignInMethod } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import TextField from '@/components/ui/TextField.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import AccountPanel from '@/features/account/AccountPanel.vue';
import { formatDate } from '@/lib/time';
import { useSessionStore } from '@/stores/session';

const props = defineProps<{ memberSince: string; signInMethod: SignInMethod }>();

const MAX_PICTURE_BYTES = 5 * 1024 * 1024;

const { t, te } = useI18n();
const session = useSessionStore();

const fileInput = ref<HTMLInputElement | null>(null);
const name = ref(session.user?.displayName ?? '');
const busy = ref(false);
const saved = ref(false);
const error = ref<string | null>(null);

const isSso = computed(() => props.signInMethod === SignInMethod.sso);
const trimmed = computed(() => name.value.trim());
const canSave = computed(
  () => !busy.value && trimmed.value !== '' && trimmed.value !== session.user?.displayName,
);
const dirty = computed(() => name.value !== session.user?.displayName);

// A rename elsewhere (or a save here) realigns the field.
watch(
  () => session.user?.displayName,
  (value) => {
    name.value = value ?? '';
  },
);

function fail(code: string): void {
  error.value = te(`errors.${code}`) ? t(`errors.${code}`) : t('errors.unknown');
}

function reset(): void {
  name.value = session.user?.displayName ?? '';
  error.value = null;
}

async function save(): Promise<void> {
  if (!canSave.value) return;
  busy.value = true;
  error.value = null;
  saved.value = false;
  try {
    const res = await api.updateMe({ displayName: trimmed.value });
    if (res.status !== 200) throw apiError(res);
    session.setUser(res.data);
    name.value = res.data.displayName;
    saved.value = true;
  } catch (e) {
    fail(errorCode(e));
  } finally {
    busy.value = false;
  }
}

function choosePicture(): void {
  fileInput.value?.click();
}

async function onFile(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  error.value = null;
  saved.value = false;
  if (file.size > MAX_PICTURE_BYTES) {
    fail('picture_too_large');
    return;
  }
  busy.value = true;
  try {
    const res = await api.putPicture(file, { headers: { 'Content-Type': file.type } });
    if (res.status !== 200) {
      // A body far over the limit can be refused without a problem document.
      throw res.status === 413 ? new Error('picture_too_large') : apiError(res);
    }
    session.setUser(res.data);
  } catch (e) {
    fail(e instanceof Error && e.message === 'picture_too_large' ? e.message : errorCode(e));
  } finally {
    busy.value = false;
  }
}

async function removePicture(): Promise<void> {
  busy.value = true;
  error.value = null;
  saved.value = false;
  try {
    const res = await api.deletePicture();
    if (res.status !== 200) throw apiError(res);
    session.setUser(res.data);
  } catch (e) {
    fail(errorCode(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AccountPanel v-if="session.user" id="profile" :title="t('account.profile.title')">
    <div class="profile">
      <AppAlert v-if="error" class="profile__alert">{{ error }}</AppAlert>
      <div class="profile__body">
        <div class="picture">
          <div class="picture__avatar">
            <UserAvatar
              :user-id="session.user.id"
              :name="session.user.displayName"
              :size="104"
              :picture-version="session.user.pictureVersion"
            />
            <button
              type="button"
              class="picture__change"
              :aria-label="t('account.profile.changePicture')"
              :disabled="busy"
              @click="choosePicture"
            >
              <AppIcon name="image" :size="16" />
            </button>
          </div>
          <input
            ref="fileInput"
            type="file"
            class="picture__input"
            accept="image/png,image/jpeg,image/gif,image/webp"
            tabindex="-1"
            aria-hidden="true"
            @change="onFile"
          />
          <button type="button" class="picture__upload" :disabled="busy" @click="choosePicture">
            <AppIcon name="image" :size="15" />
            {{ t('account.profile.uploadPicture') }}
          </button>
          <button
            v-if="session.user.pictureVersion"
            type="button"
            class="picture__remove"
            :disabled="busy"
            @click="removePicture"
          >
            {{ t('account.profile.removePicture') }}
          </button>
          <span class="picture__hint">{{ t('account.profile.pictureHint') }}</span>
        </div>

        <form class="details" @submit.prevent="save">
          <TextField
            id="display-name"
            v-model="name"
            dense
            maxlength="80"
            autocomplete="name"
            :label="t('account.profile.displayName')"
            :hint="t('account.profile.displayNameHint')"
          />

          <div class="email">
            <span class="email__label">{{ t('account.profile.email') }}</span>
            <div class="email__value" aria-describedby="email-hint">
              <AppIcon name="lock" :size="15" />
              <span>{{ session.user.email }}</span>
            </div>
            <span id="email-hint" class="email__hint">
              {{ isSso ? t('account.profile.emailHintSso') : t('account.profile.emailHintLocal') }}
            </span>
          </div>

          <div class="facts">
            <i18n-t keypath="account.profile.signedIn" tag="span">
              <template #method>
                <strong>{{
                  isSso ? t('account.profile.methodSso') : t('account.profile.methodPassword')
                }}</strong>
              </template>
            </i18n-t>
            <i18n-t keypath="account.profile.memberSince" tag="span">
              <template #date>
                <strong>{{ formatDate(memberSince) }}</strong>
              </template>
            </i18n-t>
          </div>

          <div class="actions">
            <AppButton variant="ghost" size="touch" :disabled="busy || !dirty" @click="reset">
              {{ t('account.profile.cancel') }}
            </AppButton>
            <AppButton type="submit" variant="primary" size="touch" :disabled="!canSave">
              {{ t('account.profile.save') }}
            </AppButton>
            <span v-if="saved" class="actions__saved" role="status">
              {{ t('account.profile.saved') }}
            </span>
          </div>
        </form>
      </div>
    </div>
  </AccountPanel>
</template>

<style scoped>
.profile {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 0 20px 22px;
}
.profile__body {
  display: flex;
  flex-wrap: wrap;
  gap: 28px;
  align-items: flex-start;
}
.picture {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  width: 160px;
}
.picture__avatar {
  position: relative;
}
.picture__input {
  display: none;
}
.picture__change {
  position: absolute;
  right: 0;
  bottom: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  padding: 0;
  border: 3px solid var(--surface);
  border-radius: 50%;
  background: var(--accent);
  color: var(--on-accent);
  cursor: pointer;
}
.picture__upload {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-height: 44px;
  padding: 0 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
}
.picture__upload:hover:not(:disabled) {
  background: var(--well);
}
.picture__remove {
  min-height: 44px;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--muted);
  font-family: var(--font-sans);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.picture__remove:hover:not(:disabled) {
  color: var(--text);
}
.picture button:disabled {
  cursor: progress;
  opacity: 0.75;
}
.picture__hint {
  font-size: 12px;
  color: var(--muted);
  text-align: center;
}
.details {
  flex: 1 1 360px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.email {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.email__label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}
.email__value {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 0 14px;
  border-radius: var(--radius-control);
  background: var(--well);
  color: var(--text);
  font-size: 14px;
  overflow-wrap: anywhere;
}
.email__hint {
  font-size: 12px;
  color: var(--muted);
}
.facts {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 24px;
  font-size: 13px;
  color: var(--muted);
}
.facts strong {
  color: var(--text);
}
.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}
.actions__saved {
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
}
/* Phones: the picture column spans the panel so the fields stack beneath it. */
@media (max-width: 520px) {
  .profile__body {
    gap: 20px;
  }
  .picture {
    width: 100%;
  }
}
</style>
