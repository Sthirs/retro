import createClient from 'openapi-fetch';

import type { components, paths } from './schema';

export type User = components['schemas']['UserDto'];
export type AuthConfig = components['schemas']['AuthConfig'];
export type SessionResponse = components['schemas']['SessionResponse'];
export type Problem = components['schemas']['Problem'];

/** Supplies the in-memory access token and renews it when it expires. */
export interface TokenSource {
  token(): string | null;
  /** Renews the session; resolves to whether a new token is available. */
  renew(): Promise<boolean>;
}

let tokens: TokenSource | null = null;

export function setTokenSource(source: TokenSource | null): void {
  tokens = source;
}

const AUTH_PATHS = ['/api/v1/auth/login', '/api/v1/auth/refresh', '/api/v1/auth/logout'];

/**
 * Adds `Authorization: Bearer …` and, on a 401, renews the session once and
 * retries. Auth endpoints themselves are never retried.
 */
export async function authFetch(input: Request): Promise<Response> {
  const retry = input.clone();
  const response = await fetch(withToken(input));
  const path = new URL(input.url, window.location.origin).pathname;
  if (response.status !== 401 || !tokens || AUTH_PATHS.includes(path)) return response;
  if (!(await tokens.renew())) return response;
  return fetch(withToken(retry));
}

function withToken(request: Request): Request {
  const token = tokens?.token();
  if (token) request.headers.set('Authorization', `Bearer ${token}`);
  return request;
}

export const api = createClient<paths>({
  baseUrl: window.location.origin,
  credentials: 'same-origin',
  fetch: authFetch,
});
