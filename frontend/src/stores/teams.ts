import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import {
  api,
  apiError,
  type PendingInvitationDto,
  type TeamDto,
  type TeamRefDto,
} from '@/api/client';
import { useSessionStore } from '@/stores/session';

const LAST_TEAM_KEY = 'retro.lastTeam';

function readLastTeam(userId: string): string | null {
  try {
    return localStorage.getItem(`${LAST_TEAM_KEY}.${userId}`);
  } catch {
    return null;
  }
}

function writeLastTeam(userId: string, teamId: string): void {
  try {
    localStorage.setItem(`${LAST_TEAM_KEY}.${userId}`, teamId);
  } catch {
    // Storage unavailable: the choice lasts for this page only.
  }
}

/**
 * The signed-in user's teams and pending invitations (from `GET /me`), and
 * the last used team, remembered per user in the browser (spec §7.1).
 */
export const useTeamsStore = defineStore('teams', () => {
  const session = useSessionStore();
  const teams = ref<TeamDto[]>([]);
  const invitations = ref<PendingInvitationDto[]>([]);
  const loaded = ref(false);
  let pending: Promise<void> | null = null;

  const hasTeams = computed(() => teams.value.length > 0);

  function reset(): void {
    teams.value = [];
    invitations.value = [];
    loaded.value = false;
  }

  // A different user (or none) invalidates everything.
  watch(
    () => session.user?.id,
    () => reset(),
  );

  /** Reloads teams and invitations. */
  function load(): Promise<void> {
    pending ??= (async () => {
      try {
        const res = await api.me();
        if (res.status !== 200) throw apiError(res);
        teams.value = res.data.teams;
        invitations.value = res.data.invitations;
        loaded.value = true;
      } finally {
        pending = null;
      }
    })();
    return pending;
  }

  async function ensureLoaded(): Promise<void> {
    if (!loaded.value) await load();
  }

  function byId(id: string): TeamDto | undefined {
    return teams.value.find((t) => t.id === id);
  }

  /** The team to open by default: the last used one, else the first. */
  function defaultTeam(): TeamDto | undefined {
    const userId = session.user?.id;
    const last = userId ? readLastTeam(userId) : null;
    return (last ? byId(last) : undefined) ?? teams.value[0];
  }

  function remember(teamId: string): void {
    const userId = session.user?.id;
    if (userId) writeLastTeam(userId, teamId);
  }

  async function createTeam(name: string): Promise<TeamDto> {
    const res = await api.createTeam({ name });
    if (res.status !== 201) throw apiError(res);
    await load();
    return res.data;
  }

  async function acceptInvitation(token: string): Promise<TeamRefDto> {
    const res = await api.acceptInvitation(encodeURIComponent(token));
    if (res.status !== 200) throw apiError(res);
    await load();
    return res.data;
  }

  async function declineInvitation(token: string): Promise<void> {
    const res = await api.declineInvitation(encodeURIComponent(token));
    if (res.status !== 204) throw apiError(res);
    await load();
  }

  /** Updates a team's name in the list after a rename. */
  function patch(team: TeamDto): void {
    teams.value = teams.value.map((t) => (t.id === team.id ? team : t));
  }

  return {
    teams,
    invitations,
    loaded,
    hasTeams,
    load,
    ensureLoaded,
    byId,
    defaultTeam,
    remember,
    createTeam,
    acceptInvitation,
    declineInvitation,
    patch,
    reset,
  };
});
