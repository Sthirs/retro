import { request, type APIRequestContext } from '@playwright/test';

import { PASSWORD } from './fixtures';

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
 * administers, leaves the others and declines open invitations. The e2e suite
 * owns the alice and bob accounts of the dev cluster.
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
  } finally {
    await api.dispose();
  }
}
