import { useI18n } from 'vue-i18n';

import { minutesBetween } from '@/lib/time';

/** "58 min", "1 h 12 min", "2 h": how long a meeting ran. */
export function useDuration() {
  const { t } = useI18n();
  return (startIso: string, endIso: string): string => {
    const total = minutesBetween(startIso, endIso);
    const h = Math.floor(total / 60);
    const m = total % 60;
    if (h === 0) return t('meetings.minutes', { m });
    if (m === 0) return t('meetings.hours', { h });
    return t('meetings.hoursMinutes', { h, m });
  };
}
