<script setup lang="ts">
/**
 * Invite by email (InviteEmail artboard). Opens on a freshly created
 * invitation and shows its personal link; "Invite another" creates more.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, errorCode, type InvitationDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import CopyField from '@/components/ui/CopyField.vue';
import TextField from '@/components/ui/TextField.vue';
import { relativeFromNow } from '@/lib/time';

const props = defineProps<{ open: boolean; teamId: string; created: InvitationDto | null }>();
const emit = defineEmits<{ close: []; created: [InvitationDto] }>();
const { t, locale } = useI18n();

const invitation = ref<InvitationDto | null>(props.created);
const email = ref('');
const busy = ref(false);
const error = ref<string | null>(null);

watch(
  () => [props.open, props.created] as const,
  ([open, created]) => {
    if (!open) return;
    invitation.value = created;
    email.value = created?.email ?? '';
    error.value = null;
  },
);

function another(): void {
  invitation.value = null;
  email.value = '';
  error.value = null;
}

async function create(): Promise<void> {
  if (busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    const res = await api.createInvitation(props.teamId, { email: email.value });
    if (res.status !== 201) throw apiError(res);
    invitation.value = res.data;
    emit('created', res.data);
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('settings.inviteDialog.title')" @close="emit('close')">
    <form id="invite-form" class="form" novalidate @submit.prevent="create">
      <TextField
        id="ie"
        v-model="email"
        dense
        type="email"
        autocomplete="off"
        :readonly="!!invitation"
        :autofocus="!invitation"
        :label="t('settings.inviteDialog.email')"
        :hint="t('settings.inviteDialog.hint')"
        :error="error ? t(`errors.${error}`) : undefined"
      />
    </form>
    <div v-if="invitation" class="created" data-testid="invitation-created">
      <span class="created__title">
        <AppIcon name="check" :size="17" :stroke-width="2.5" />
        {{ t('settings.inviteDialog.created') }}
      </span>
      <span class="created__text">
        {{
          t('settings.inviteDialog.createdText', {
            email: invitation.email,
            when: relativeFromNow(invitation.expiresAt, locale),
          })
        }}
      </span>
      <CopyField
        id="invitation-link"
        :value="invitation.url ?? ''"
        :label="t('settings.inviteDialog.link')"
        :show-label="false"
        :button-label="t('settings.inviteDialog.copyLink')"
        variant="primary"
        dense
      />
    </div>
    <template #footer>
      <template v-if="invitation">
        <AppButton variant="ghost" @click="emit('close')">{{
          t('settings.inviteDialog.done')
        }}</AppButton>
        <AppButton @click="another">{{ t('settings.inviteDialog.another') }}</AppButton>
      </template>
      <template v-else>
        <AppButton variant="ghost" @click="emit('close')">{{ t('common.cancel') }}</AppButton>
        <AppButton variant="primary" type="submit" form="invite-form" :disabled="busy">
          {{ t('settings.inviteDialog.create') }}
        </AppButton>
      </template>
    </template>
  </AppDialog>
</template>

<style scoped>
.form {
  display: contents;
}
.created {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  border-radius: var(--radius-control);
  background: var(--col-plus-bg);
}
.created__title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  font-weight: 700;
  color: var(--col-plus-fg);
}
.created__text {
  font-size: 13px;
}
</style>
