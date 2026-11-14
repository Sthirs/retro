/**
 * Dev accounts. alice and bob are seeded in the backend and in dex (password or
 * single sign-on); dana, erin and finn are seeded in the backend only (password
 * only): dana is used by the account spec, erin and finn by the tasks spec;
 * carol exists only in dex (single sign-on only).
 * See backend/src/seed.rs and deploy/helm/retro/templates/dev-dex.yaml.
 */
export const USERS = {
  alice: { username: 'alice', email: 'alice@retro.local', name: 'Alice Rossi' },
  bob: { username: 'bob', email: 'bob@retro.local', name: 'Bob Bianchi' },
  dana: { username: 'dana', email: 'dana@retro.local', name: 'Dana Neri' },
  erin: { username: 'erin', email: 'erin@retro.local', name: 'Erin Gallo' },
  finn: { username: 'finn', email: 'finn@retro.local', name: 'Finn Moretti' },
  carol: { username: 'carol', email: 'carol@retro.local', name: 'Carol Verdi' },
} as const;

export const PASSWORD = 'password';
