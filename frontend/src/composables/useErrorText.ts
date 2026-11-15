import { useI18n } from 'vue-i18n';

/**
 * Translates an API error code (spec §14) for display. Codes without a
 * message of their own (for example `internal` or `unavailable`) fall back to
 * the generic `errors.unknown`, so a raw key is never shown.
 */
export function useErrorText(): (code: string) => string {
  const { t, te } = useI18n();
  return (code) => (te(`errors.${code}`) ? t(`errors.${code}`) : t('errors.unknown'));
}
