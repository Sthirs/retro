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

/**
 * The fetch used by the generated client (orval mutator). Resolves to
 * `{ data, status, headers }`; `data` is the parsed JSON body, or `undefined`
 * when the response has no JSON body (e.g. 204, or an HTML error page).
 */
export async function apiFetch<T>(url: string, init: RequestInit): Promise<T> {
  const request = new Request(new URL(url, window.location.origin), {
    credentials: 'same-origin',
    ...init,
  });
  const response = await authFetch(request);
  const isJson = response.headers.get('Content-Type')?.includes('json') ?? false;
  const text = isJson ? await response.text() : '';
  const data: unknown = text ? JSON.parse(text) : undefined;
  return { data, status: response.status, headers: response.headers } as T;
}
