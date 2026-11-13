/** A failed API call, carrying the problem's stable `code` (spec §14). */
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
  ) {
    super(code);
  }
}

/** Builds an ApiError from a generated-client response that was not a success. */
export function apiError(res: { status: number; data?: unknown }): ApiError {
  const data = res.data as { code?: unknown } | undefined;
  const code = typeof data?.code === 'string' ? data.code : 'unknown';
  return new ApiError(res.status, code);
}

/** The error code to explain to the user, defaulting to `unknown`. */
export function errorCode(error: unknown): string {
  return error instanceof ApiError ? error.code : 'unknown';
}
