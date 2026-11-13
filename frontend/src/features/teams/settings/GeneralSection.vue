<script setup lang="ts">
/** Team name; editable by admins (spec §5.2). */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, errorCode, type TeamDto } from '@/api/client';
import AppButton from '@/components/ui/AppButton.vue';
import TextField from '@/components/ui/TextField.vue';
import SettingsPanel from '@/features/teams/settings/SettingsPanel.vue';
import { useTeamsStore } from '@/stores/teams';

const props = defineProps<{ team: TeamDto; isAdmin: boolean }>();
const { t } = useI18n();
const teams = useTeamsStore();

const name = ref(props.team.name);
const busy = ref(false);
const saved = ref(false);
const error = ref<string | null>(null);

watch(
  () => props.team.name,
  (value) => (name.value = value),
);

async function save(): Promise<void> {
  if (busy.value) return;
  if (!name.value.trim()) {
    error.value = 'invalid_team_name';
    return;
  }
  busy.value = true;
  error.value = null;
  saved.value = false;
  try {
    const res = await api.renameTeam(props.team.id, { name: name.value });
    if (res.status !== 200) throw apiError(res);
    teams.patch(res.data);
    saved.value = true;
  } catch (e) {
    error.value = errorCode(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <SettingsPanel id="gs" :title="t('settings.general')">
    <form v-if="isAdmin" class="row" novalidate @submit.prevent="save">
      <div class="row__field">
        <TextField
          id="tn"
          v-model="name"
          dense
          maxlength="60"
          autocomplete="off"
          :label="t('settings.teamName')"
          :error="error ? t(`errors.${error}`) : undefined"
          @input="saved = false"
        />
      </div>
      <AppButton type="submit" variant="primary" size="touch" :disabled="busy">
        {{ t('common.save') }}
      </AppButton>
      <span class="row__status" role="status">{{ saved ? t('common.saved') : '' }}</span>
    </form>
    <dl v-else class="readonly">
      <dt>{{ t('settings.teamName') }}</dt>
      <dd>{{ team.name }}</dd>
    </dl>
  </SettingsPanel>
</template>

<style scoped>
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 12px;
  padding: 0 20px 20px;
}
.row__field {
  flex: 1 1 320px;
}
.row__status {
  align-self: center;
  font-size: 13px;
  color: var(--muted);
}
.row__status:empty {
  display: none;
}
.readonly {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0 20px 20px;
}
.readonly dt {
  font-size: 13px;
  font-weight: 600;
}
.readonly dd {
  margin: 0;
}
</style>
