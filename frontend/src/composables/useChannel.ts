import { onScopeDispose, toValue, watch, type MaybeRefOrGetter } from 'vue';

import { realtime, type RealtimeHandler } from '@/api/realtime';

/**
 * Listens to a real-time channel while the calling component (or scope) lives.
 * A `null` channel listens to nothing; a changed channel moves the subscription.
 */
export function useChannel(
  channel: MaybeRefOrGetter<string | null>,
  handler: RealtimeHandler,
): void {
  let stop: (() => void) | null = null;
  watch(
    () => toValue(channel),
    (name) => {
      stop?.();
      stop = name ? realtime.subscribe(name, handler) : null;
    },
    { immediate: true },
  );
  onScopeDispose(() => stop?.());
}
