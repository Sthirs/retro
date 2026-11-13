<script setup lang="ts">
/** `/join/:joinToken`: joins the team behind the link (spec §6.2). */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute } from 'vue-router';

import { api, type TeamRefDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import AppButton from '@/components/ui/AppButton.vue';
import LinkLayout from '@/features/invitations/LinkLayout.vue';
import LinkStateCard from '@/features/invitations/LinkStateCard.vue';
import { useTeamsStore } from '@/stores/teams';

type State =
  | { kind: 'joining' }
  | { kind: 'joined'; team: TeamRefDto }
  | { kind: 'invalid' }
  | { kind: 'error'; code: string };

const { t } = useI18n();
const route = useRoute();
const teams = useTeamsStore();
const state = ref<State>({ kind: 'joining' });

onMounted(async () => {
  try {
    const res = await api.joinTeam(encodeURIComponent(String(route.params.joinToken)));
    if (res.status === 200) {
      await teams.load();
      state.value = { kind: 'joined', team: res.data.team };
    } else if (res.status === 404) {
      state.value = { kind: 'invalid' };
    } else {
      state.value = { kind: 'error', code: res.status === 429 ? 'too_many_requests' : 'unknown' };
    }
  } catch {
    state.value = { kind: 'error', code: 'unknown' };
  }
});
</script>

<template>
  <LinkLayout>
    <p v-if="state.kind === 'joining'" class="busy" aria-busy="true">
      {{ t('linkStates.joining') }}
    </p>
    <LinkStateCard
      v-else-if="state.kind === 'joined'"
      tone="success"
      icon="check"
      :title="t('linkStates.joinedTitle', { team: state.team.name })"
      :text="t('linkStates.joinedText')"
    >
      <AppButton
        variant="primary"
        :to="{ name: 'team-meetings', params: { teamId: state.team.id } }"
      >
        {{ t('linkStates.goToMeetings') }}
        <AppIcon name="arrowRight" :size="16" />
      </AppButton>
    </LinkStateCard>
    <LinkStateCard
      v-else-if="state.kind === 'invalid'"
      icon="alert"
      :title="t('linkStates.joinInvalidTitle')"
      :text="t('linkStates.joinInvalidText')"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>
    <LinkStateCard
      v-else
      icon="alert"
      :title="t('linkStates.joinInvalidTitle')"
      :text="t(`errors.${state.code}`)"
    >
      <AppButton to="/">{{ t('common.backToRetro') }}</AppButton>
    </LinkStateCard>
  </LinkLayout>
</template>

<style scoped>
.busy {
  color: var(--muted);
}
</style>
