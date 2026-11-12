import { fileURLToPath, URL } from 'node:url';

import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vitest/config';

const devPort = Number(process.env.FRONTEND_PORT ?? 5173);
const devHost = process.env.RETRO_DEV_HOST;
const hmrClientPort = Number(process.env.VITE_HMR_CLIENT_PORT ?? devPort);
const backendUrl = process.env.BACKEND_URL ?? 'http://localhost:3000';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: devPort,
    // Behind the Minikube ingress (Telepresence intercept), HMR must go
    // through the public host on port 80.
    allowedHosts: devHost ? [devHost] : undefined,
    hmr: devHost ? { host: devHost, clientPort: hmrClientPort, protocol: 'ws' } : undefined,
    proxy: {
      '/api': { target: backendUrl, changeOrigin: false },
    },
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    include: ['tests/**/*.test.ts'],
    restoreMocks: true,
  },
});
