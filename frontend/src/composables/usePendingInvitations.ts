import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';

import { errorCode } from '@/api/client';
import { useTeamsStore } from '@/stores/teams';

/**
 * Accepting and declining the signed-in user's pending invitations (spec §7.1),
 * shared by the user menu and the account page. Accepting opens the joined team.
 */
export function usePendingInvitations(options: { onAccepted?: () => void } = {}) {
  const router = useRouter();
  const teams = useTeamsStore();

  const invitations = computed(() => teams.invitations);
  /** Token of the invitation being answered. */
  const busy = ref<string | null>(null);
  /** Error code of the last failed answer. */
  const error = ref<string | null>(null);

  async function answer(token: string, action: () => Promise<void>): Promise<void> {
    busy.value = token;
    error.value = null;
    try {
      await action();
    } catch (e) {
      error.value = errorCode(e);
      // The invitation may be gone or expired: refresh the list.
      await teams.load().catch(() => undefined);
    } finally {
      busy.value = null;
    }
  }

  function accept(token: string): Promise<void> {
    return answer(token, async () => {
      const team = await teams.acceptInvitation(token);
      options.onAccepted?.();
      await router.push({ name: 'team-meetings', params: { teamId: team.id } });
    });
  }

  function decline(token: string): Promise<void> {
    return answer(token, () => teams.declineInvitation(token));
  }

  return { invitations, busy, error, accept, decline };
}
