import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { api, setTokenSource, type Problem, type SessionResponse, type User } from '@/api/client';

/** Error codes the sign-in page knows how to explain. */
export type LoginErrorCode = 'invalid_credentials' | 'too_many_requests' | 'unknown';

export class LoginError extends Error {
  constructor(public readonly code: LoginErrorCode) {
    super(code);
  }
}

/**
 * The signed-in user. The access token lives only in memory (spec §3); the
 * httpOnly refresh cookie restores the session after a reload.
 */
export const useSessionStore = defineStore('session', () => {
  const accessToken = ref<string | null>(null);
  const user = ref<User | null>(null);
  /** True once the initial restore attempt has finished. */
  const ready = ref(false);
  let pendingRefresh: Promise<boolean> | null = null;

  const isAuthenticated = computed(() => user.value !== null);

  function apply(session: SessionResponse): void {
    accessToken.value = session.accessToken;
    user.value = session.user;
  }

  function clear(): void {
    accessToken.value = null;
    user.value = null;
  }

  async function login(login: string, password: string, remember: boolean): Promise<void> {
    const { data, error, response } = await api.POST('/api/v1/auth/login', {
      body: { login, password, remember },
    });
    if (data) {
      apply(data);
      return;
    }
    const code = (error as Problem | undefined)?.code;
    if (response.status === 401 && code === 'invalid_credentials') {
      throw new LoginError('invalid_credentials');
    }
    if (response.status === 429) throw new LoginError('too_many_requests');
    throw new LoginError('unknown');
  }

  /** Renews the session from the refresh cookie. Concurrent calls share one request. */
  function refresh(): Promise<boolean> {
    pendingRefresh ??= (async () => {
      try {
        const { data } = await api.POST('/api/v1/auth/refresh');
        if (data) {
          apply(data);
          return true;
        }
        clear();
        return false;
      } catch {
        clear();
        return false;
      } finally {
        pendingRefresh = null;
      }
    })();
    return pendingRefresh;
  }

  /** First-load restore, run once before the first navigation. */
  async function restore(): Promise<void> {
    if (ready.value) return;
    await refresh();
    ready.value = true;
  }

  async function logout(): Promise<void> {
    try {
      await api.POST('/api/v1/auth/logout');
    } finally {
      clear();
    }
  }

  setTokenSource({ token: () => accessToken.value, renew: refresh });

  return { accessToken, user, ready, isAuthenticated, login, refresh, restore, logout };
});
