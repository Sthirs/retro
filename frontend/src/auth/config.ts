import { api, type AuthConfig } from '@/api/client';

/** Used when the backend cannot be reached: offer password sign-in only. */
export const FALLBACK_AUTH_CONFIG: AuthConfig = {
  localLogin: true,
  sso: { enabled: false, buttonText: '' },
  passwordResetUrl: null,
};

export async function fetchAuthConfig(): Promise<AuthConfig> {
  try {
    const res = await api.config();
    return res.status === 200 && res.data ? res.data : FALLBACK_AUTH_CONFIG;
  } catch {
    return FALLBACK_AUTH_CONFIG;
  }
}

/** Starts single sign-on; the backend redirects to the identity provider. */
export function ssoUrl(redirect: string, remember: boolean): string {
  const params = new URLSearchParams({ redirect });
  if (remember) params.set('remember', 'true');
  return `/api/v1/auth/oidc?${params.toString()}`;
}
