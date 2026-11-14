<script setup lang="ts">
/** "Your teams": every membership with its role, plus Open, Leave and Create team. */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';

import type { TeamDto } from '@/api/client';
import AppIcon from '@/components/AppIcon.vue';
import CreateTeamDialog from '@/components/layout/CreateTeamDialog.vue';
import TeamTile from '@/components/TeamTile.vue';
import AppButton from '@/components/ui/AppButton.vue';
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue';
import { useLeaveTeam } from '@/composables/useLeaveTeam';
import AccountPanel from '@/features/account/AccountPanel.vue';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

const { t } = useI18n();
const session = useSessionStore();
const teams = useTeamsStore();
const leave = useLeaveTeam();

const creating = ref(false);

function askLeave(team: TeamDto): void {
  leave.ask(team, team.memberCount === 1);
}

async function confirmLeave(): Promise<void> {
  if (session.user) await leave.confirm(session.user.id);
}
</script>

<template>
  <AccountPanel
    id="teams"
    :title="t('account.teams.title', { count: teams.teams.length })"
    :description="t('account.teams.hint')"
  >
    <template #actions>
      <AppButton size="touch" @click="creating = true">
        <AppIcon name="plus" :size="16" />
        {{ t('account.teams.create') }}
      </AppButton>
    </template>
    <ul class="list">
      <li v-for="team in teams.teams" :key="team.id" class="row" data-testid="team-row">
        <TeamTile :name="team.name" :size="40" />
        <div class="row__text">
          <span class="row__name">
            {{ team.name }}
            <span v-if="team.id === teams.defaultTeam()?.id" class="tag tag--current">
              {{ t('account.teams.current') }}
            </span>
          </span>
          <span class="row__meta">
            {{ t('account.teams.members', { count: team.memberCount }, team.memberCount) }}
          </span>
        </div>
        <span class="tag" :class="team.role === 'admin' ? 'tag--admin' : 'tag--member'">
          {{ team.role === 'admin' ? t('common.admin') : t('common.member') }}
        </span>
        <div class="row__actions">
          <AppButton
            variant="ghost"
            size="touch"
            :to="{ name: 'team-meetings', params: { teamId: team.id } }"
            :aria-label="t('account.teams.openLabel', { team: team.name })"
          >
            {{ t('account.teams.open') }}
          </AppButton>
          <AppButton
            variant="ghost"
            size="touch"
            :aria-label="t('account.teams.leaveLabel', { team: team.name })"
            @click="askLeave(team)"
          >
            <AppIcon name="logout" :size="15" />
            {{ t('account.teams.leave') }}
          </AppButton>
        </div>
      </li>
    </ul>
    <ConfirmDialog
      :open="!!leave.pending.value"
      :title="leave.dialog.value?.title ?? ''"
      :text="leave.dialog.value?.text ?? ''"
      :confirm-label="leave.dialog.value?.confirm ?? ''"
      :busy="leave.busy.value"
      :error="leave.error.value"
      danger
      @close="leave.cancel()"
      @confirm="confirmLeave"
    />
    <CreateTeamDialog :open="creating" @close="creating = false" />
  </AccountPanel>
</template>

<style scoped>
.list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 14px;
  padding: 12px 20px;
  border-top: 1px solid var(--line);
}
.row__text {
  display: flex;
  flex: 1 1 200px;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.row__name {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 15px;
  font-weight: 700;
}
.row__meta {
  font-size: 13px;
  color: var(--muted);
}
.row__actions {
  display: flex;
  gap: 4px;
}
/* Artboard: 13 px row actions; the height stays at the 44 px touch target. */
.row__actions :deep(.btn) {
  font-size: 13px;
}
.tag {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}
.tag--admin {
  background: var(--accent-soft);
  color: var(--accent-text);
}
.tag--member {
  background: var(--well);
  color: var(--muted);
}
.tag--current {
  background: var(--accent);
  color: var(--on-accent);
}
</style>
