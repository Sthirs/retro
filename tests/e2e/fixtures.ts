/**
 * Dev accounts. alice and bob are seeded in the backend and in dex (password or
 * single sign-on); dana is seeded in the backend only (password only) and is
 * used by the account spec; carol exists only in dex (single sign-on only).
 * See backend/src/seed.rs and deploy/helm/retro/templates/dev-dex.yaml.
 */
export const USERS = {
  alice: { username: 'alice', email: 'alice@retro.local', name: 'Alice Rossi' },
  bob: { username: 'bob', email: 'bob@retro.local', name: 'Bob Bianchi' },
  dana: { username: 'dana', email: 'dana@retro.local', name: 'Dana Neri' },
  carol: { username: 'carol', email: 'carol@retro.local', name: 'Carol Verdi' },
} as const;

export const PASSWORD = 'password';
