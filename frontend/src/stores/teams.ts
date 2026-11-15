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
const DEFAULT_TEAM_KEY = 'retro.defaultTeam';

function readStored(key: string, userId: string): string | null {
  try {
    return localStorage.getItem(`${key}.${userId}`);
  } catch {
    return null;
  }
}

function writeStored(key: string, userId: string, value: string | null): void {
  try {
    if (value) localStorage.setItem(`${key}.${userId}`, value);
    else localStorage.removeItem(`${key}.${userId}`);
  } catch {
    // Storage unavailable: the choice lasts for this page only.
  }
}

/**
 * The signed-in user's teams and pending invitations (from `GET /me`), and
 * the team opened by default: a pinned one, else the last used one, both
 * remembered per user in the browser (spec §7.1).
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

  // The current user's last used and pinned teams, reactive so views follow `remember` and `pin`.
  const lastTeam = ref<string | null>(null);
  const pinnedTeam = ref<string | null>(null);
  watch(
    () => session.user?.id,
    (userId) => {
      lastTeam.value = userId ? readStored(LAST_TEAM_KEY, userId) : null;
      pinnedTeam.value = userId ? readStored(DEFAULT_TEAM_KEY, userId) : null;
    },
    { immediate: true, flush: 'sync' },
  );

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

  /**
   * The team to open by default: the pinned one while the user is still in it,
   * else the last used one, else the first.
   */
  function defaultTeam(): TeamDto | undefined {
    const pinned = pinnedTeam.value;
    const last = lastTeam.value;
    return (pinned ? byId(pinned) : undefined) ?? (last ? byId(last) : undefined) ?? teams.value[0];
  }

  /** The team the user is in outside team pages (the account page): the last used one, else the first. */
  function currentTeam(): TeamDto | undefined {
    const last = lastTeam.value;
    return (last ? byId(last) : undefined) ?? teams.value[0];
  }

  /** The pinned default team, if the user is still a member of it. */
  const pinnedDefault = computed(() => (pinnedTeam.value ? byId(pinnedTeam.value) : undefined));

  function remember(teamId: string): void {
    const userId = session.user?.id;
    if (!userId) return;
    lastTeam.value = teamId;
    writeStored(LAST_TEAM_KEY, userId, teamId);
  }

  /** Pins the team opened by default; `null` goes back to the last used one. */
  function pin(teamId: string | null): void {
    const userId = session.user?.id;
    if (!userId) return;
    pinnedTeam.value = teamId;
    writeStored(DEFAULT_TEAM_KEY, userId, teamId);
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
    currentTeam,
    pinnedDefault,
    remember,
    pin,
    createTeam,
    acceptInvitation,
    declineInvitation,
    patch,
    reset,
  };
});
