import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  mockFetch,
  problem,
  renderAt,
  session,
  team,
  TEAM_ID,
  type FakeRoute,
} from './helpers';

const base = `/api/v1/teams/${TEAM_ID}`;

/** Submits the form that contains the input matched by `selector`. */
async function submitFormOf(wrapper: VueWrapper, selector: string): Promise<void> {
  const input = wrapper.find<HTMLInputElement>(selector).element;
  input.form?.dispatchEvent(new Event('submit'));
  await flushPromises();
}

const member = (user: typeof ALICE, role: 'admin' | 'member') => ({
  user,
  role,
  joinedAt: '2026-11-01T10:00:00Z',
});

const invitation = (overrides: Record<string, unknown> = {}) => ({
  id: 'inv-1',
  email: 'nina@acme.dev',
  url: 'https://retro.test/invite/abc',
  invitedBy: { id: BOB.id, displayName: 'Bob Bianchi' },
  expiresAt: new Date(Date.now() + 6 * 24 * 3600 * 1000).toISOString(),
  ...overrides,
});

function routes(
  role: 'admin' | 'member',
  extra: Record<string, FakeRoute | (() => FakeRoute)> = {},
): Record<string, FakeRoute | (() => FakeRoute)> {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team({ role })]),
    [`GET ${base}/members`]: {
      status: 200,
      body: [member(BOB, 'admin'), member(ALICE, role)],
    },
    [`GET ${base}/invitations`]: { status: 200, body: [invitation()] },
    [`GET ${base}/join-link`]: {
      status: 200,
      body: { url: 'https://retro.test/join/jt', enabled: true },
    },
    ...extra,
  };
}

describe('TeamSettingsView', () => {
  it('shows every section to admins', async () => {
    mockFetch(routes('admin'));
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);

    expect(wrapper.find('h1').text()).toBe('Team settings');
    expect(wrapper.text()).toContain("Platform Team · you're an admin");
    expect((wrapper.find('#tn').element as HTMLInputElement).value).toBe('Platform Team');
    expect(wrapper.text()).toContain(
      "Retro doesn't send emails: create an invitation and share its link, or share the team's join link.",
    );
    expect((wrapper.find('#jl').element as HTMLInputElement).value).toBe(
      'https://retro.test/join/jt',
    );
    expect(wrapper.find('[role="switch"]').attributes('aria-checked')).toBe('true');
    expect(wrapper.text()).toContain('Regenerate link');
    expect(wrapper.text()).toContain('Pending invitations · 1');
    expect(wrapper.text()).toContain('Invited by Bob Bianchi · expires in 6 days');
    expect(wrapper.text()).toContain('Members · 2');
    // You first, then the others.
    const rows = wrapper.findAll('[data-testid="member"]');
    expect(rows[0]?.text()).toContain('Alice Rossi (you)');
    expect(rows[0]?.text()).toContain('Leave team');
    expect(rows[1]?.text()).toContain('Make member');
    expect(wrapper.find('button[aria-label="Remove Bob Bianchi"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('Delete team');
  });

  it('hides admin actions from members', async () => {
    mockFetch(routes('member'));
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);

    expect(wrapper.text()).toContain("Platform Team · you're a member");
    expect(wrapper.find('#tn').exists()).toBe(false);
    expect(wrapper.find('[role="switch"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('Regenerate link');
    expect(wrapper.text()).not.toContain('Make member');
    expect(wrapper.find('button[aria-label="Remove Bob Bianchi"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('Delete team');
    // Members can still invite and copy the join link, but not revoke others' invitations.
    expect(wrapper.find('#inv-email').exists()).toBe(true);
    expect(wrapper.find('#jl').exists()).toBe(true);
    expect(wrapper.text()).not.toContain('Revoke');
  });

  it("does not offer another member's invitation link", async () => {
    mockFetch(
      routes('member', {
        [`GET ${base}/invitations`]: {
          status: 200,
          body: [
            invitation({ url: null }),
            invitation({
              id: 'inv-2',
              email: 'omar@acme.dev',
              invitedBy: { id: ALICE.id, displayName: 'Alice Rossi' },
            }),
          ],
        },
      }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    const rows = wrapper.findAll('[data-testid="pending-invitation"]');
    expect(rows).toHaveLength(2);
    expect(rows[0]?.text()).not.toContain('Copy link');
    expect(rows[1]?.text()).toContain('Copy link');
  });

  it('explains that another member already invited that email', async () => {
    mockFetch(routes('member', { [`POST ${base}/invitations`]: problem(409, 'already_invited') }));
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper.find('#inv-email').setValue('nina@acme.dev');
    await submitFormOf(wrapper, '#inv-email');
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain('This email already has an open invitation.'),
    );
  });

  it('renames the team', async () => {
    const { calls } = mockFetch(
      routes('admin', {
        [`PATCH ${base}`]: { status: 200, body: team({ name: 'Core Team' }) },
      }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper.find('#tn').setValue('Core Team');
    await submitFormOf(wrapper, '#tn');
    await vi.waitFor(() => expect(wrapper.text()).toContain("Core Team · you're an admin"));
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({ name: 'Core Team' });
  });

  it('creates an invitation and shows its personal link', async () => {
    const { calls } = mockFetch(
      routes('admin', {
        [`POST ${base}/invitations`]: {
          status: 201,
          body: invitation({
            id: 'inv-2',
            email: 'omar@acme.dev',
            url: 'https://retro.test/invite/xyz',
            invitedBy: { id: ALICE.id, displayName: 'Alice Rossi' },
          }),
        },
      }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper.find('#inv-email').setValue('omar@acme.dev');
    await submitFormOf(wrapper, '#inv-email');
    await vi.waitFor(() =>
      expect(wrapper.find('[data-testid="invitation-created"]').exists()).toBe(true),
    );
    expect(calls.find((c) => c.method === 'POST' && c.path.endsWith('/invitations'))?.body).toEqual(
      {
        email: 'omar@acme.dev',
      },
    );
    expect(wrapper.text()).toContain('Invitation created');
    expect((wrapper.find('#invitation-link').element as HTMLInputElement).value).toBe(
      'https://retro.test/invite/xyz',
    );
  });

  it('shows why an invitation cannot be created', async () => {
    mockFetch(routes('admin', { [`POST ${base}/invitations`]: problem(409, 'already_member') }));
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper.find('#inv-email').setValue('bob@retro.local');
    await submitFormOf(wrapper, '#inv-email');
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain('This person is already a member of the team.'),
    );
  });

  it('revokes an invitation', async () => {
    const { calls } = mockFetch(
      routes('admin', { [`DELETE ${base}/invitations/inv-1`]: { status: 204 } }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper
      .find('button[aria-label="Revoke the invitation for nina@acme.dev"]')
      .trigger('click');
    await vi.waitFor(() =>
      expect(
        calls.some((c) => c.method === 'DELETE' && c.path.endsWith('/invitations/inv-1')),
      ).toBe(true),
    );
  });

  it('turns the join link off and regenerates it', async () => {
    const { calls } = mockFetch(
      routes('admin', {
        [`POST ${base}/join-link`]: {
          status: 200,
          body: { url: 'https://retro.test/join/jt', enabled: false },
        },
      }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper.find('[role="switch"]').trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.find('[role="switch"]').attributes('aria-checked')).toBe('false'),
    );
    expect(calls.find((c) => c.path.endsWith('/join-link') && c.method === 'POST')?.body).toEqual({
      action: 'disable',
    });
  });

  it('reports the last-admin rule', async () => {
    mockFetch(
      routes('admin', {
        [`PATCH ${base}/members/${BOB.id}`]: problem(409, 'last_admin'),
      }),
    );
    const { wrapper } = await renderAt(`/teams/${TEAM_ID}/settings`);
    const button = wrapper.findAll('button').find((b) => b.text() === 'Make member');
    await button?.trigger('click');
    await vi.waitFor(() =>
      expect(wrapper.text()).toContain('Every team needs at least one admin. Make someone else'),
    );
  });

  it('deletes the team only after its name is typed', async () => {
    let deleted = false;
    const { calls } = mockFetch(
      routes('admin', {
        'GET /api/v1/me': () => me(deleted ? [] : [team()]),
        [`DELETE ${base}`]: () => {
          deleted = true;
          return { status: 204 };
        },
      }),
    );
    const { wrapper, router } = await renderAt(`/teams/${TEAM_ID}/settings`);
    await wrapper
      .findAll('button')
      .find((b) => b.text() === 'Delete team')
      ?.trigger('click');
    await vi.waitFor(() => expect(wrapper.find('#dc').exists()).toBe(true));

    const confirm = () => wrapper.findAll('button').find((b) => b.text() === 'Delete team forever');
    expect(confirm()?.attributes('disabled')).toBeDefined();
    await wrapper.find('#dc').setValue('Platform');
    expect(confirm()?.attributes('disabled')).toBeDefined();
    await wrapper.find('#dc').setValue('Platform Team');
    expect(confirm()?.attributes('disabled')).toBeUndefined();

    await submitFormOf(wrapper, '#dc');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('home'));
    expect(calls.some((c) => c.method === 'DELETE' && c.path === base)).toBe(true);
  });
});
