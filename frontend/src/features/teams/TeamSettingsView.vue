<script setup lang="ts">
/** Team settings (spec §5.2): name, invitations, join link, members, deletion. */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useRoute, useRouter } from 'vue-router';

import { api, apiError, type InvitationDto, type JoinLinkDto, type MemberDto } from '@/api/client';
import AppAlert from '@/components/ui/AppAlert.vue';
import DeleteSection from '@/features/teams/settings/DeleteSection.vue';
import GeneralSection from '@/features/teams/settings/GeneralSection.vue';
import InviteSection from '@/features/teams/settings/InviteSection.vue';
import MembersSection from '@/features/teams/settings/MembersSection.vue';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const session = useSessionStore();
const teams = useTeamsStore();

const teamId = computed(() => String(route.params.teamId));
const team = computed(() => teams.byId(teamId.value));
const isAdmin = computed(() => team.value?.role === 'admin');
const members = ref<MemberDto[]>([]);
const invitations = ref<InvitationDto[]>([]);
const joinLink = ref<JoinLinkDto | null>(null);
const loaded = ref(false);
const loadError = ref(false);

async function loadMembers(): Promise<void> {
  const res = await api.listMembers(teamId.value);
  if (res.status !== 200) throw apiError(res);
  members.value = res.data;
}

async function loadInvitations(): Promise<void> {
  const res = await api.listInvitations(teamId.value);
  if (res.status !== 200) throw apiError(res);
  invitations.value = res.data;
}

async function loadJoinLink(): Promise<void> {
  const res = await api.getJoinLink(teamId.value);
  if (res.status !== 200) throw apiError(res);
  joinLink.value = res.data;
}

/** Reloads everything; roles may have changed, so the team list too. */
async function refresh(): Promise<void> {
  try {
    await Promise.all([teams.load(), loadMembers(), loadInvitations(), loadJoinLink()]);
    loadError.value = false;
  } catch {
    loadError.value = true;
  }
  loaded.value = true;
  if (!team.value) await router.replace({ name: 'home' });
}

onMounted(refresh);
</script>

<template>
  <div v-if="team && session.user" class="settings">
    <div class="settings__header">
      <h1 class="settings__title">{{ t('settings.title') }}</h1>
      <p class="settings__subtitle">
        {{
          isAdmin
            ? t('settings.subtitleAdmin', { team: team.name })
            : t('settings.subtitleMember', { team: team.name })
        }}
      </p>
    </div>

    <AppAlert v-if="loadError">{{ t('errors.unknown') }}</AppAlert>

    <GeneralSection :team="team" :is-admin="isAdmin" />
    <template v-if="loaded">
      <InviteSection
        :team-id="team.id"
        :is-admin="isAdmin"
        :current-user-id="session.user.id"
        :invitations="invitations"
        :join-link="joinLink"
        @changed="loadInvitations"
        @join-link="joinLink = $event"
      />
      <MembersSection
        :team="team"
        :members="members"
        :is-admin="isAdmin"
        :current-user-id="session.user.id"
        @changed="refresh"
      />
      <DeleteSection
        v-if="isAdmin"
        :team="team"
        :member-count="members.length"
        :invitation-count="invitations.length"
      />
    </template>
  </div>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
  width: 100%;
  max-width: 920px;
}
.settings__header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.settings__title {
  margin: 0;
  font-size: 28px;
  font-weight: 700;
  letter-spacing: -0.01em;
}
.settings__subtitle {
  margin: 0;
  color: var(--muted);
}
</style>
