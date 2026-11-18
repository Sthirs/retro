/**
 * Dev accounts. alice and bob are seeded in the backend and in dex (password or
 * single sign-on) and own the demo teams; the suite never touches them, nor
 * carol, who exists only in dex (single sign-on only). dana, erin, finn, gina,
 * hugo, ivan, jade, kai, lena, milo, nora, omar, pia, quinn, rosa, sami, tess, uma,
 * vera, walt and xena are seeded in the backend only (password only) and belong to
 * the suite: dana
 * to the account spec, erin and finn to the tasks spec, gina and hugo to the
 * teams spec, ivan, jade and kai to the meetings spec, lena, milo and nora to
 * the reflect spec, omar and pia to the group spec, quinn, rosa, sami and tess
 * to the vote spec, uma, vera, walt and xena to the discuss spec. See
 * backend/src/seed.rs and deploy/helm/retro/templates/dev-dex.yaml.
 */
export const USERS = {
  alice: { username: 'alice', email: 'alice@retro.local', name: 'Alice Rossi' },
  bob: { username: 'bob', email: 'bob@retro.local', name: 'Bob Bianchi' },
  dana: { username: 'dana', email: 'dana@retro.local', name: 'Dana Neri' },
  erin: { username: 'erin', email: 'erin@retro.local', name: 'Erin Gallo' },
  finn: { username: 'finn', email: 'finn@retro.local', name: 'Finn Moretti' },
  gina: { username: 'gina', email: 'gina@retro.local', name: 'Gina Ferri' },
  hugo: { username: 'hugo', email: 'hugo@retro.local', name: 'Hugo Conti' },
  ivan: { username: 'ivan', email: 'ivan@retro.local', name: 'Ivan Galli' },
  jade: { username: 'jade', email: 'jade@retro.local', name: 'Jade Marino' },
  kai: { username: 'kai', email: 'kai@retro.local', name: 'Kai Russo' },
  lena: { username: 'lena', email: 'lena@retro.local', name: 'Lena Fontana' },
  milo: { username: 'milo', email: 'milo@retro.local', name: 'Milo Testa' },
  nora: {
    username: 'nora',
    email: 'nora@retro.local',
    name: 'Nora Pellegrini',
  },
  omar: { username: 'omar', email: 'omar@retro.local', name: 'Omar Ricci' },
  pia: { username: 'pia', email: 'pia@retro.local', name: 'Pia Longo' },
  quinn: { username: 'quinn', email: 'quinn@retro.local', name: 'Quinn Serra' },
  rosa: { username: 'rosa', email: 'rosa@retro.local', name: 'Rosa Villa' },
  sami: { username: 'sami', email: 'sami@retro.local', name: 'Sami Caruso' },
  tess: { username: 'tess', email: 'tess@retro.local', name: 'Tess Barbieri' },
  uma: { username: 'uma', email: 'uma@retro.local', name: 'Uma Sartori' },
  vera: { username: 'vera', email: 'vera@retro.local', name: 'Vera Mancini' },
  walt: { username: 'walt', email: 'walt@retro.local', name: 'Walt Greco' },
  xena: { username: 'xena', email: 'xena@retro.local', name: 'Xena Bruno' },
  carol: { username: 'carol', email: 'carol@retro.local', name: 'Carol Verdi' },
} as const;

export const PASSWORD = 'password';
