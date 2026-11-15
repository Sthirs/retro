import { ref } from 'vue';
import { useRouter } from 'vue-router';

import { api, apiError, errorCode } from '@/api/client';

/**
 * "Add meeting" (spec §7.2): starts a retrospective for the team and takes
 * the caller into its room, as its facilitator.
 */
export function useStartMeeting() {
  const router = useRouter();
  const busy = ref(false);
  const error = ref<string | null>(null);

  async function start(teamId: string): Promise<void> {
    if (busy.value) return;
    busy.value = true;
    error.value = null;
    try {
      const res = await api.startMeeting(teamId);
      if (res.status !== 201) throw apiError(res);
      await router.push({
        name: 'meeting-room',
        params: { teamId, meetingId: res.data.id },
      });
    } catch (e) {
      error.value = errorCode(e);
    } finally {
      busy.value = false;
    }
  }

  return { start, busy, error };
}
