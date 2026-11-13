<script setup lang="ts">
/**
 * Delete team (admin, spec §5.1): confirmed by typing the team name; deletes
 * everything in the team.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { api, apiError, errorCode, type TeamDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppAlert from '@/components/ui/AppAlert.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import TextField from '@/components/ui/TextField.vue';
import { useTeamsStore } from '@/stores/teams';

const props = defineProps<{ team: TeamDto; memberCount: number; invitationCount: number }>();
const { t } = useI18n();
const router = useRouter();
const teams = useTeamsStore();

const open = ref(false);
const typed = ref('');
const busy = ref(false);
const error = ref<string | null>(null);

const matches = computed(() => typed.value.trim() === props.team.name);

function start(): void {
  typed.value = '';
  error.value = null;
  open.value = true;
}

async function remove(): Promise<void> {
  if (!matches.value || busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    const res = await api.deleteTeam(props.team.id);
    if (res.status !== 204) throw apiError(res);
    open.value = false;
    await teams.load();
    await router.push({ name: 'home' });
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="danger" aria-labelledby="dz">
    <div class="danger__text">
      <h2 id="dz" class="danger__title">{{ t('settings.delete.title') }}</h2>
      <p class="danger__body">{{ t('settings.delete.text') }}</p>
    </div>
    <AppButton variant="danger-outline" @click="start">
      <AppIcon name="trash" :size="16" />
      {{ t('settings.delete.button') }}
    </AppButton>

    <AppDialog
      :open="open"
      :title="t('settings.delete.dialogTitle', { team: team.name })"
      @close="open = false"
    >
      <template #icon>
        <span class="dialog-icon" aria-hidden="true"><AppIcon name="trash" :size="20" /></span>
      </template>
      <i18n-t keypath="settings.delete.dialogText" tag="p" class="dialog-text">
        <template #team
          ><strong class="dialog-text__team">{{ team.name }}</strong></template
        >
        <template #memberships>{{ t('settings.delete.memberships', memberCount) }}</template>
        <template #invitations>{{ t('settings.delete.invitations', invitationCount) }}</template>
      </i18n-t>
      <form id="delete-form" novalidate @submit.prevent="remove">
        <TextField
          id="dc"
          v-model="typed"
          dense
          autofocus
          autocomplete="off"
          :label="t('settings.delete.confirmLabel')"
        />
      </form>
      <AppAlert v-if="error">{{ t(`errors.${error}`) }}</AppAlert>
      <template #footer>
        <AppButton variant="ghost" @click="open = false">{{ t('common.cancel') }}</AppButton>
        <AppButton variant="danger" type="submit" form="delete-form" :disabled="!matches || busy">
          {{ t('settings.delete.confirm') }}
        </AppButton>
      </template>
    </AppDialog>
  </section>
</template>

<style scoped>
.danger {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 18px 20px;
  border: 1px solid var(--danger);
  border-radius: var(--radius-panel);
}
.danger__text {
  display: flex;
  flex: 1 1 360px;
  flex-direction: column;
  gap: 2px;
}
.danger__title {
  margin: 0;
  font-size: 17px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.danger__body {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
}
.dialog-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: var(--col-delta-bg);
  color: var(--danger);
}
.dialog-text {
  margin: 0;
  color: var(--muted);
}
.dialog-text__team {
  color: var(--text);
}
</style>
