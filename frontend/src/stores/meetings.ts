import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import { api, apiError, errorCode, MeetingStatus, type MeetingSummaryDto } from '@/api/client';
import { useSessionStore } from '@/stores/session';

/** Coalesces bursts of team events (join + presence) into one reload. */
const RELOAD_DELAY_MS = 150;

/**
 * The Meetings page's data: a team's active and ended meetings (spec §7.2),
 * reloaded whenever the team channel reports a meeting change.
 */
export const useMeetingsStore = defineStore('meetings', () => {
  const session = useSessionStore();
  const teamId = ref<string | null>(null);
  const meetings = ref<MeetingSummaryDto[]>([]);
  const loaded = ref(false);
  const error = ref<string | null>(null);
  let loadSeq = 0;
  let reloadTimer: ReturnType<typeof setTimeout> | null = null;

  const active = computed(() => meetings.value.filter((m) => m.status === MeetingStatus.active));
  /** Most recently ended first. */
  const ended = computed(() =>
    meetings.value
      .filter((m) => m.status === MeetingStatus.ended)
      .sort((a, b) => (b.endedAt ?? '').localeCompare(a.endedAt ?? '')),
  );

  function reset(): void {
    loadSeq++;
    if (reloadTimer) clearTimeout(reloadTimer);
    reloadTimer = null;
    teamId.value = null;
    meetings.value = [];
    loaded.value = false;
    error.value = null;
  }

  watch(
    () => session.user?.id,
    () => reset(),
  );

  /** Loads a team's meetings; switching team clears the previous list first. */
  async function load(id: string): Promise<void> {
    if (teamId.value !== id) {
      reset();
      teamId.value = id;
    }
    const seq = ++loadSeq;
    try {
      const res = await api.listMeetings(id);
      if (res.status !== 200) throw apiError(res);
      if (seq !== loadSeq) return;
      meetings.value = res.data;
      loaded.value = true;
      error.value = null;
    } catch (e) {
      if (seq === loadSeq) error.value = errorCode(e);
    }
  }

  /** Reloads the current team's meetings soon (after a real-time event). */
  function reloadSoon(): void {
    const id = teamId.value;
    if (!id || reloadTimer) return;
    reloadTimer = setTimeout(() => {
      reloadTimer = null;
      if (teamId.value === id) void load(id);
    }, RELOAD_DELAY_MS);
  }

  return { teamId, meetings, active, ended, loaded, error, load, reloadSoon, reset };
});
