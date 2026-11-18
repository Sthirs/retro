import { request, type APIRequestContext } from '@playwright/test';

import { PASSWORD, USERS } from './fixtures';

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:5173';

async function signedIn(login: string): Promise<{ api: APIRequestContext; userId: string }> {
  const anonymous = await request.newContext({
    baseURL: BASE_URL,
    ignoreHTTPSErrors: true,
  });
  const res = await anonymous.post('/api/v1/auth/login', {
    data: { login, password: PASSWORD, remember: false },
  });
  if (!res.ok()) throw new Error(`login ${login}: ${res.status()}`);
  const { accessToken, user } = (await res.json()) as {
    accessToken: string;
    user: { id: string };
  };
  await anonymous.dispose();
  const api = await request.newContext({
    baseURL: BASE_URL,
    ignoreHTTPSErrors: true,
    extraHTTPHeaders: { Authorization: `Bearer ${accessToken}` },
  });
  return { api, userId: user.id };
}

/**
 * Brings a dev account back to "no team, no invitation": deletes the teams it
 * administers, leaves the others, declines open invitations and restores the
 * display name and picture. Only for accounts the suite owns (see fixtures.ts),
 * and only where the test needs that empty state (onboarding) or restores what
 * it changed; never for alice or bob, who own the demo data. Tests that only
 * need a team create their own with `createTeam` and delete it with `deleteTeam`.
 */
export async function resetAccount(login: string): Promise<void> {
  const { api, userId } = await signedIn(login);
  try {
    const me = (await (await api.get('/api/v1/me')).json()) as {
      teams: { id: string; role: string }[];
      invitations: { token: string }[];
    };
    for (const team of me.teams) {
      if (team.role === 'admin') await api.delete(`/api/v1/teams/${team.id}`);
      else await api.delete(`/api/v1/teams/${team.id}/members/${userId}`);
    }
    for (const invitation of me.invitations) {
      await api.post(`/api/v1/invitations/${invitation.token}/decline`);
    }
    // The account page lets people rename themselves and upload a picture.
    const known = Object.values(USERS).find((user) => user.username === login);
    if (known) await api.patch('/api/v1/me', { data: { displayName: known.name } });
    await api.delete('/api/v1/me/picture'); // 200 with or without a picture
  } finally {
    await api.dispose();
  }
}

/** Creates a team with `login` as its admin, and returns its id. */
export async function createTeam(login: string, name: string): Promise<string> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post('/api/v1/teams', { data: { name } });
    if (!res.ok()) throw new Error(`create team: ${res.status()} ${await res.text()}`);
    return ((await res.json()) as { id: string }).id;
  } finally {
    await api.dispose();
  }
}

/** Deletes a team that `login` administers; a team already gone is fine. */
export async function deleteTeam(login: string, teamId: string): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.delete(`/api/v1/teams/${teamId}`);
    if (!res.ok() && res.status() !== 404) {
      throw new Error(`delete team: ${res.status()} ${await res.text()}`);
    }
  } finally {
    await api.dispose();
  }
}

/** The id of a dev account (signs in to read it). */
export async function userIdOf(login: string): Promise<string> {
  const { api, userId } = await signedIn(login);
  await api.dispose();
  return userId;
}

/**
 * Adds a task at the end of a column through the dev-only seeding route (the
 * dev cluster runs with SEED_DEV_USERS). `login` is the caller, a team member;
 * `authorId` defaults to the caller. `content` is rich-text HTML.
 */
export async function seedTask(
  login: string,
  teamId: string,
  task: {
    status: 'future' | 'stuck' | 'active' | 'done';
    content: string;
    authorId?: string;
    /** A source meeting of the team (see `startMeeting`). */
    meetingId?: string;
  },
): Promise<{ id: string }> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/dev/teams/${teamId}/tasks`, {
      data: task,
    });
    if (!res.ok()) throw new Error(`seed task: ${res.status()} ${await res.text()}`);
    return (await res.json()) as { id: string };
  } finally {
    await api.dispose();
  }
}

/** Starts a meeting in the team, with `login` as its facilitator; returns its id. */
export async function startMeeting(login: string, teamId: string): Promise<string> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/teams/${teamId}/meetings`);
    if (!res.ok()) throw new Error(`start meeting: ${res.status()} ${await res.text()}`);
    return ((await res.json()) as { id: string }).id;
  } finally {
    await api.dispose();
  }
}

/** Removes `userId` from the team (`login` is an admin, or the user leaving). */
export async function removeMember(login: string, teamId: string, userId: string): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.delete(`/api/v1/teams/${teamId}/members/${userId}`);
    if (res.status() !== 204) throw new Error(`remove member: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** The path of a running meeting's invite link (`/m/{token}`), as a member copies it. */
export async function meetingInvitePath(login: string, meetingId: string): Promise<string> {
  const { api } = await signedIn(login);
  try {
    const res = await api.get(`/api/v1/meetings/${meetingId}/invite-link`);
    if (!res.ok()) throw new Error(`invite link: ${res.status()} ${await res.text()}`);
    return new URL(((await res.json()) as { url: string }).url).pathname;
  } finally {
    await api.dispose();
  }
}

/** Renames a running meeting (`login` is its facilitator). */
export async function renameMeeting(
  login: string,
  meetingId: string,
  title: string,
): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.patch(`/api/v1/meetings/${meetingId}`, {
      data: { title },
    });
    if (!res.ok()) throw new Error(`rename meeting: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** Adds `login` to the team through its join link (`admin` reads the link). */
export async function joinTeam(admin: string, teamId: string, login: string): Promise<void> {
  const { api: adminApi } = await signedIn(admin);
  let token: string;
  try {
    const res = await adminApi.get(`/api/v1/teams/${teamId}/join-link`);
    if (!res.ok()) throw new Error(`join link: ${res.status()} ${await res.text()}`);
    token = new URL(((await res.json()) as { url: string }).url).pathname.split('/').pop()!;
  } finally {
    await adminApi.dispose();
  }
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/join/${token}`);
    if (!res.ok()) throw new Error(`join team: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** Changes a member's team role (`admin` is a team admin). */
export async function setRole(
  admin: string,
  teamId: string,
  userId: string,
  role: 'admin' | 'member',
): Promise<void> {
  const { api } = await signedIn(admin);
  try {
    const res = await api.patch(`/api/v1/teams/${teamId}/members/${userId}`, {
      data: { role },
    });
    if (!res.ok()) throw new Error(`set role: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** POSTs `/meetings/{id}/{action}` as `login` (e.g. `join`, `phase/next`). */
export async function meetingAction(
  login: string,
  meetingId: string,
  action: string,
): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/meetings/${meetingId}/${action}`);
    if (!res.ok()) throw new Error(`${action}: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** Sends a reflection as `login` (a participant, during Reflect). */
export async function writeReflection(
  login: string,
  meetingId: string,
  column: 'plus' | 'delta' | 'ideas' | 'thanks',
  content: string,
): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/meetings/${meetingId}/reflections`, {
      data: { column, content },
    });
    if (!res.ok()) throw new Error(`write reflection: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** The parts of the meeting state the specs check. */
export interface MeetingState {
  phase: string;
  reflections: { id: string; content: string; mine: boolean; groupId: string | null }[];
  groups: { id: string; column: string; name: string }[];
  voteBudget: number;
  voteMaxPerGroup: number;
  /** The viewer's own votes only. */
  myVotes: { groupId: string; count: number }[];
  /** The facilitator's anonymous view of everyone's votes during Vote, else null. */
  voteUsage: { spent: number; maxOnGroup: number }[] | null;
}

/** The meeting state as `login` sees it. */
export async function meetingState(login: string, meetingId: string): Promise<MeetingState> {
  const { api } = await signedIn(login);
  try {
    const res = await api.get(`/api/v1/meetings/${meetingId}/state`);
    if (!res.ok()) throw new Error(`state: ${res.status()} ${await res.text()}`);
    return (await res.json()) as MeetingState;
  } finally {
    await api.dispose();
  }
}

/** Drops the card `reflectionId` onto the card `targetId` (Group); returns the group id. */
export async function groupCards(
  login: string,
  meetingId: string,
  targetId: string,
  reflectionId: string,
): Promise<string> {
  const { api } = await signedIn(login);
  try {
    const res = await api.post(`/api/v1/meetings/${meetingId}/groups`, {
      data: { targetId, reflectionId },
    });
    if (!res.ok())
      throw new Error(`group cards: ${res.status()} ${await res.text()}`);
    const state = (await res.json()) as MeetingState;
    return state.reflections.find((r) => r.id === targetId)!.groupId!;
  } finally {
    await api.dispose();
  }
}

/** Names a group (Group). */
export async function renameGroup(
  login: string,
  groupId: string,
  name: string,
): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.patch(`/api/v1/groups/${groupId}`, {
      data: { name },
    });
    if (!res.ok())
      throw new Error(`rename group: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}

/** Adds (1) or takes back (-1) a vote as `login`; returns the status and error code. */
export async function castVote(
  login: string,
  groupId: string,
  delta: number,
): Promise<{ status: number; code: string | null }> {
  const { api } = await signedIn(login);
  try {
    const res = await api.put(`/api/v1/groups/${groupId}/vote`, {
      data: { delta },
    });
    const body = (await res.json()) as { code?: string };
    return {
      status: res.status(),
      code: res.ok() ? null : (body.code ?? null),
    };
  } finally {
    await api.dispose();
  }
}

/** Sets the vote budget and the most votes per group (`login` facilitates). */
export async function setVoteSettings(
  login: string,
  meetingId: string,
  voteBudget: number,
  voteMaxPerGroup: number,
): Promise<void> {
  const { api } = await signedIn(login);
  try {
    const res = await api.patch(`/api/v1/meetings/${meetingId}/settings`, {
      data: { voteBudget, voteMaxPerGroup },
    });
    if (!res.ok())
      throw new Error(`vote settings: ${res.status()} ${await res.text()}`);
  } finally {
    await api.dispose();
  }
}
