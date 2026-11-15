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
