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
 * display name and picture. The e2e suite
 * owns the alice, bob, dana, erin and finn accounts of the dev cluster (dana,
 * erin and finn are backend-seeded only; dana is used by the account spec, erin
 * and finn only by the tasks spec).
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
