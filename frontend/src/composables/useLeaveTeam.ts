import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';

import { api, apiError, errorCode, type TeamRefDto } from '@/api/client';
import { useTeamsStore } from '@/stores/teams';

/**
 * Leaving a team (spec §5.1): confirmation dialog state and the request. A sole
 * member leaving deletes the team; the last admin is protected by the server.
 * The caller decides where to go afterwards.
 */
export function useLeaveTeam() {
  const { t } = useI18n();
  const teams = useTeamsStore();

  const pending = ref<(TeamRefDto & { soleMember: boolean }) | null>(null);
  /** Translated message of the last failed attempt. */
  const error = ref<string | null>(null);
  const busy = ref(false);

  const dialog = computed(() => {
    const team = pending.value;
    if (!team) return null;
    return team.soleMember
      ? {
          title: t('settings.leave.lastTitle', { team: team.name }),
          text: t('settings.leave.lastText'),
          confirm: t('settings.leave.lastConfirm'),
        }
      : {
          title: t('settings.leave.title', { team: team.name }),
          text: t('settings.leave.text'),
          confirm: t('settings.leave.confirm'),
        };
  });

  function ask(team: TeamRefDto, soleMember: boolean): void {
    error.value = null;
    pending.value = { id: team.id, name: team.name, soleMember };
  }

  function cancel(): void {
    pending.value = null;
  }

  /** Removes `userId` (the signed-in user) from the team. True when they left. */
  async function confirm(userId: string): Promise<boolean> {
    const team = pending.value;
    if (!team) return false;
    busy.value = true;
    error.value = null;
    try {
      const res = await api.removeMember(team.id, userId);
      if (res.status !== 204) throw apiError(res);
      pending.value = null;
      await teams.load();
      return true;
    } catch (e) {
      error.value = t(`errors.${errorCode(e)}`);
      return false;
    } finally {
      busy.value = false;
    }
  }

  return { pending, dialog, error, busy, ask, confirm, cancel };
}
