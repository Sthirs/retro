/** Accounts seeded in dev mode, in both the backend and dex (see seed.rs, dev-dex.yaml). */
export const USERS = {
  alice: { username: 'alice', email: 'alice@retro.local', name: 'Alice Rossi' },
  bob: { username: 'bob', email: 'bob@retro.local', name: 'Bob Bianchi' },
} as const;

export const PASSWORD = 'password';
