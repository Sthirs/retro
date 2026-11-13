import { defineConfig } from 'orval';

// Generates the typed API client from the backend's OpenAPI document
// (`npm run api:generate` from the repository root).
export default defineConfig({
  retro: {
    input: { target: './openapi.json' },
    output: {
      target: './src/api/generated.ts',
      client: 'fetch',
      mode: 'single',
      override: { mutator: { path: './src/api/fetcher.ts', name: 'apiFetch' } },
    },
  },
});
