import { computed, onScopeDispose, ref, toValue, watch, type MaybeRefOrGetter } from 'vue';

import { TimerStatus, type Timer } from '@/api/client';
import { formatClock } from '@/lib/clock';

/** How often a running countdown refreshes. */
const TICK_MS = 250;

/**
 * The meeting timer's countdown (spec §8.2), on server time: `serverTime`
 * corrects the local clock with the offset of the last `serverNow` seen.
 * It ticks only while the timer runs.
 */
export function useCountdown(
  timer: MaybeRefOrGetter<Timer | null | undefined>,
  serverTime: () => number,
) {
  const now = ref(serverTime());
  let handle: ReturnType<typeof setInterval> | undefined;

  function stop(): void {
    if (handle !== undefined) clearInterval(handle);
    handle = undefined;
  }

  watch(
    () => toValue(timer),
    (t) => {
      stop();
      now.value = serverTime();
      if (t?.status === TimerStatus.running) {
        handle = setInterval(() => (now.value = serverTime()), TICK_MS);
      }
    },
    { immediate: true, deep: true },
  );
  onScopeDispose(stop);

  const remainingMs = computed(() => {
    const t = toValue(timer);
    if (!t) return 0;
    if (t.status === TimerStatus.paused) return t.remainingMs ?? 0;
    return t.endsAt ? Math.max(0, Date.parse(t.endsAt) - now.value) : 0;
  });
  /** Running and at zero. */
  const expired = computed(() => {
    const t = toValue(timer);
    return t?.status === TimerStatus.running && remainingMs.value === 0;
  });
  const display = computed(() => formatClock(remainingMs.value));

  return { remainingMs, expired, display };
}
