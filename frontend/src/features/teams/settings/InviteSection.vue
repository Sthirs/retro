<script setup lang="ts">
/**
 * Invite people (spec §6): invitations by email with a personal link, the
 * team join link, and pending invitations. No email is ever sent.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, errorCode, type InvitationDto, type JoinLinkDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppSwitch from '@/components/ui/AppSwitch.vue';
import CopyField from '@/components/ui/CopyField.vue';
import TextField from '@/components/ui/TextField.vue';
import InviteDialog from '@/features/teams/settings/InviteDialog.vue';
import SettingsPanel from '@/features/teams/settings/SettingsPanel.vue';
import { copyText } from '@/lib/clipboard';
import { relativeFromNow } from '@/lib/time';

const props = defineProps<{
  teamId: string;
  isAdmin: boolean;
  currentUserId: string;
  invitations: InvitationDto[];
  joinLink: JoinLinkDto | null;
}>();
const emit = defineEmits<{ changed: []; joinLink: [JoinLinkDto] }>();
const { t, locale } = useI18n();

const email = ref('');
const creating = ref(false);
const emailError = ref<string | null>(null);
const dialogOpen = ref(false);
const created = ref<InvitationDto | null>(null);
const linkBusy = ref(false);
const error = ref<string | null>(null);
const copiedId = ref<string | null>(null);

async function createInvitation(): Promise<void> {
  if (creating.value) return;
  creating.value = true;
  emailError.value = null;
  try {
    const res = await api.createInvitation(props.teamId, { email: email.value });
    if (res.status !== 201) throw apiError(res);
    created.value = res.data;
    dialogOpen.value = true;
    email.value = '';
    emit('changed');
  } catch (e) {
    emailError.value = errorCode(e);
  } finally {
    creating.value = false;
  }
}

async function updateLink(action: 'regenerate' | 'enable' | 'disable'): Promise<void> {
  linkBusy.value = true;
  error.value = null;
  try {
    const res = await api.updateJoinLink(props.teamId, { action });
    if (res.status !== 200) throw apiError(res);
    emit('joinLink', res.data);
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    linkBusy.value = false;
  }
}

function setEnabled(enabled: boolean): void {
  void updateLink(enabled ? 'enable' : 'disable');
}

async function copyInvitation(invitation: InvitationDto): Promise<void> {
  if (invitation.url && (await copyText(invitation.url))) {
    copiedId.value = invitation.id;
    setTimeout(() => {
      if (copiedId.value === invitation.id) copiedId.value = null;
    }, 2000);
  }
}

async function revoke(invitation: InvitationDto): Promise<void> {
  error.value = null;
  try {
    const res = await api.revokeInvitation(props.teamId, invitation.id);
    if (res.status !== 204) throw apiError(res);
  } catch (e) {
    error.value = errorCode(e);
  }
  emit('changed');
}

function canRevoke(invitation: InvitationDto): boolean {
  return props.isAdmin || invitation.invitedBy?.id === props.currentUserId;
}

function meta(invitation: InvitationDto): string {
  const when = relativeFromNow(invitation.expiresAt, locale.value);
  if (invitation.invitedBy?.id === props.currentUserId) {
    return t('settings.pending.invitedByYou', { when });
  }
  if (invitation.invitedBy) {
    return t('settings.pending.invitedBy', { name: invitation.invitedBy.displayName, when });
  }
  return t('settings.pending.expires', { when });
}
</script>

<template>
  <SettingsPanel
    id="is"
    :title="t('settings.invite.title')"
    :description="t('settings.invite.text')"
  >
    <form class="invite" novalidate @submit.prevent="createInvitation">
      <div class="invite__field">
        <TextField
          id="inv-email"
          v-model="email"
          dense
          type="email"
          autocomplete="off"
          :label="t('settings.invite.email')"
          :placeholder="t('settings.invite.emailPlaceholder')"
          :hint="t('settings.invite.emailHint')"
          :error="emailError ? t(`errors.${emailError}`) : undefined"
        />
      </div>
      <AppButton
        type="submit"
        variant="primary"
        size="touch"
        class="invite__submit"
        :disabled="creating"
      >
        <AppIcon name="userPlus" :size="17" />
        {{ t('settings.invite.create') }}
      </AppButton>
    </form>

    <div class="subhead">
      <h3 class="subhead__title">{{ t('settings.joinLink.title') }}</h3>
    </div>
    <div class="join">
      <CopyField
        v-if="joinLink?.url && (joinLink.enabled || isAdmin)"
        id="jl"
        :value="joinLink.url"
        :label="t('settings.joinLink.label')"
      />
      <p v-if="joinLink && !joinLink.enabled && !isAdmin" class="join__note">
        {{ t('settings.joinLink.off') }}
      </p>
      <template v-if="isAdmin && joinLink">
        <div class="join__controls">
          <AppSwitch
            :model-value="joinLink.enabled"
            :label="t('settings.joinLink.toggle')"
            :disabled="linkBusy"
            @update:model-value="setEnabled"
          />
          <AppButton variant="ghost" :disabled="linkBusy" @click="updateLink('regenerate')">
            <AppIcon name="refresh" :size="16" />
            {{ t('settings.joinLink.regenerate') }}
          </AppButton>
        </div>
        <p class="join__note">{{ t('settings.joinLink.hint') }}</p>
      </template>
    </div>

    <AppAlert v-if="error" class="error">{{ t(`errors.${error}`) }}</AppAlert>

    <template v-if="invitations.length">
      <div class="subhead">
        <h3 class="subhead__title">
          {{ t('settings.pending.title', { count: invitations.length }) }}
        </h3>
      </div>
      <ul class="list">
        <li
          v-for="invitation in invitations"
          :key="invitation.id"
          class="item"
          data-testid="pending-invitation"
        >
          <span class="item__icon" aria-hidden="true"><AppIcon name="mail" :size="17" /></span>
          <div class="item__text">
            <span class="item__email">{{ invitation.email }}</span>
            <span class="item__meta">{{ meta(invitation) }}</span>
          </div>
          <AppButton
            v-if="invitation.url"
            variant="ghost"
            size="compact"
            :aria-label="t('settings.pending.copyLabel', { email: invitation.email })"
            @click="copyInvitation(invitation)"
          >
            <AppIcon :name="copiedId === invitation.id ? 'check' : 'copy'" :size="15" />
            {{ copiedId === invitation.id ? t('common.copied') : t('settings.pending.copyLink') }}
          </AppButton>
          <AppButton
            v-if="canRevoke(invitation)"
            variant="ghost"
            size="compact"
            class="item__revoke"
            :aria-label="t('settings.pending.revokeLabel', { email: invitation.email })"
            @click="revoke(invitation)"
          >
            {{ t('settings.pending.revoke') }}
          </AppButton>
        </li>
      </ul>
    </template>

    <InviteDialog
      :open="dialogOpen"
      :team-id="teamId"
      :created="created"
      @created="emit('changed')"
      @close="dialogOpen = false"
    />
  </SettingsPanel>
</template>

<style scoped>
.invite {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: 10px;
  padding: 0 20px 18px;
}
.invite__field {
  flex: 1 1 320px;
}
.invite__submit {
  /* Line up with the input under its label. */
  margin-top: 25px;
}
.subhead {
  padding: 14px 20px 8px;
  border-top: 1px solid var(--line);
}
.subhead__title {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
}
.join {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 0 20px 20px;
}
.join__controls {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.join__note {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
}
.error {
  margin: 0 20px 18px;
  width: auto;
}
.list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  padding: 10px 20px;
  border-top: 1px solid var(--line);
}
.item__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: var(--well);
  color: var(--muted);
}
.item__text {
  display: flex;
  flex: 1 1 200px;
  flex-direction: column;
  min-width: 0;
}
.item__email {
  font-size: 14px;
  font-weight: 700;
  overflow-wrap: anywhere;
}
.item__meta {
  font-size: 13px;
  color: var(--muted);
}
.item__revoke {
  color: var(--danger);
}
.item__revoke:hover:not(:disabled) {
  color: var(--danger);
}
</style>
