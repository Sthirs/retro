import '@fontsource-variable/plus-jakarta-sans';
import './styles/tokens.css';
import './styles/base.css';

import { createPinia } from 'pinia';
import { createApp } from 'vue';

import App from './App.vue';
import { buildI18n } from './i18n';
import { buildRouter } from './router';

const app = createApp(App);
app.use(createPinia());
app.use(buildI18n());
app.use(buildRouter());
app.mount('#app');
