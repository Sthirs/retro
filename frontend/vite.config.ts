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
  build: {
    // Small assets are inlined as `data:` URIs by default, but the CSP
    // (nginx.conf) allows fonts from 'self' only: a small font subset would
    // be blocked. Fonts always ship as files.
    assetsInlineLimit: (file) => (/\.(woff2?|ttf|otf|eot)$/.test(file) ? false : undefined),
  },
  server: {
    port: devPort,
    // Behind the Minikube ingress (Telepresence intercept), HMR must go
    // through the public HTTPS host on port 443.
    allowedHosts: devHost ? [devHost] : undefined,
    hmr: devHost ? { host: devHost, clientPort: hmrClientPort, protocol: 'wss' } : undefined,
    proxy: {
      // `ws`: the real-time socket (/api/v1/ws) goes through the proxy too.
      '/api': { target: backendUrl, changeOrigin: false, ws: true },
    },
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    include: ['tests/**/*.test.ts'],
    restoreMocks: true,
  },
});
