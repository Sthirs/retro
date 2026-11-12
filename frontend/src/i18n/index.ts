import { createI18n } from 'vue-i18n';

import en from './en';

export type MessageSchema = typeof en;

export function buildI18n() {
  return createI18n<[MessageSchema], 'en'>({
    legacy: false,
    locale: 'en',
    fallbackLocale: 'en',
    messages: { en },
  });
}
