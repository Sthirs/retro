<script setup lang="ts">
/** "Add team" from the team selector (spec §7.1): create, then open the team. */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';

import { errorCode } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppDialog from '@/components/ui/AppDialog.vue';
import TextField from '@/components/ui/TextField.vue';
import { useTeamsStore } from '@/stores/teams';

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const router = useRouter();
const teams = useTeamsStore();

const name = ref('');
const error = ref<string | null>(null);
const busy = ref(false);

watch(
  () => props.open,
  (open) => {
    if (open) {
      name.value = '';
      error.value = null;
    }
  },
);

async function submit(): Promise<void> {
  if (busy.value) return;
  if (!name.value.trim()) {
    error.value = 'invalid_team_name';
    return;
  }
  busy.value = true;
  error.value = null;
  try {
    const team = await teams.createTeam(name.value);
    emit('close');
    await router.push({ name: 'team-meetings', params: { teamId: team.id } });
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <AppDialog :open="open" :title="t('createTeam.title')" :width="460" @close="emit('close')">
    <form id="create-team-form" class="form" novalidate @submit.prevent="submit">
      <TextField
        id="new-team"
        v-model="name"
        dense
        autofocus
        maxlength="60"
        autocomplete="off"
        :label="t('createTeam.name')"
        :placeholder="t('createTeam.placeholder')"
        :error="error ? t(`errors.${error}`) : undefined"
      />
      <div class="note">
        <AppIcon name="shield" :size="17" />
        <span>{{ t('createTeam.note') }}</span>
      </div>
    </form>
    <template #footer>
      <AppButton variant="ghost" @click="emit('close')">{{ t('common.cancel') }}</AppButton>
      <AppButton variant="primary" type="submit" form="create-team-form" :disabled="busy">
        {{ busy ? t('createTeam.creating') : t('createTeam.submit') }}
      </AppButton>
    </template>
  </AppDialog>
</template>

<style scoped>
.form {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.note {
  display: flex;
  gap: 10px;
  padding: 12px 14px;
  border-radius: var(--radius-control);
  background: var(--well);
  color: var(--muted);
  font-size: 13px;
}
</style>
