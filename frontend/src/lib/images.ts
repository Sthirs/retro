import { authFetch } from '@/api/fetcher';

/** Object URLs by source URL, kept for the session. Values are promises so concurrent callers share one request. */
const cache = new Map<string, Promise<string>>();
/** Bumped by `clearImageCache` so loads that were in flight do not leak an object URL. */
let generation = 0;

/**
 * Fetches an image with the bearer token (an `<img src>` cannot carry it) and
 * resolves to a `blob:` URL. Cached per URL until `clearImageCache`; rejects on
 * a non-200 response.
 */
export function loadImage(url: string): Promise<string> {
  const cached = cache.get(url);
  if (cached) return cached;
  const started = generation;
  const pending = (async () => {
    const response = await authFetch(
      new Request(new URL(url, window.location.origin), { credentials: 'same-origin' }),
    );
    if (response.status !== 200) throw new Error(`image ${url}: HTTP ${response.status}`);
    const objectUrl = URL.createObjectURL(await response.blob());
    if (started !== generation) URL.revokeObjectURL(objectUrl);
    return objectUrl;
  })();
  cache.set(url, pending);
  // A failure is not cached, so a later render can retry.
  pending.catch(() => {
    if (cache.get(url) === pending) cache.delete(url);
  });
  return pending;
}

/** Revokes every object URL and forgets them; called when the session ends. */
export function clearImageCache(): void {
  generation += 1;
  for (const pending of cache.values()) {
    pending.then(
      (objectUrl) => URL.revokeObjectURL(objectUrl),
      () => undefined,
    );
  }
  cache.clear();
}
