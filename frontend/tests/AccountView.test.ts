import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useThemePreference } from '@/lib/theme';

import {
  ALICE,
  me,
  mockFetch,
  OTHER_TEAM_ID,
  pendingInvitation,
  problem,
  renderAt,
  session,
  team,
  TEAM_ID,
  type FakeRoute,
} from './helpers';

function base(extra: Record<string, FakeRoute | (() => FakeRoute)> = {}) {
  return {
    'POST /api/v1/auth/refresh': session(),
    'GET /api/v1/me': me([team()]),
    ...extra,
  };
}

const withPicture = { ...ALICE, pictureVersion: 'v1' };

async function pickFile(wrapper: Awaited<ReturnType<typeof renderAt>>['wrapper'], file: File) {
  const input = wrapper.find('input[type="file"]');
  Object.defineProperty(input.element, 'files', { value: [file], configurable: true });
  await input.trigger('change');
  await flushPromises();
}

function button(wrapper: Awaited<ReturnType<typeof renderAt>>['wrapper'], text: string) {
  return wrapper.findAll('button').find((b) => b.text() === text);
}

describe('account page: profile', () => {
  beforeEach(() => localStorage.clear());

  it('saves a new display name', async () => {
    const { calls } = mockFetch(
      base({
        'PATCH /api/v1/me': { status: 200, body: { ...ALICE, displayName: 'Ali' } },
      }),
    );
    const { wrapper } = await renderAt('/account');
    await wrapper.find('#display-name').setValue('Ali');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.path).toBe('/api/v1/me');
    expect(patch?.body).toEqual({ displayName: 'Ali' });
    expect(wrapper.find('[data-testid="current-user"]').text()).toBe('Ali');
  });

  it('Save is disabled until the name changes', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    const save = () => button(wrapper, 'Save changes');
    expect(save()?.attributes('disabled')).toBeDefined();
    await wrapper.find('#display-name').setValue('Ali');
    expect(save()?.attributes('disabled')).toBeUndefined();
    await wrapper.find('#display-name').setValue('   ');
    expect(save()?.attributes('disabled')).toBeDefined();
    await wrapper.find('#display-name').setValue('Ali');
    await button(wrapper, 'Cancel')?.trigger('click');
    expect((wrapper.find('#display-name').element as HTMLInputElement).value).toBe(
      ALICE.displayName,
    );
    expect(save()?.attributes('disabled')).toBeDefined();
  });

  it('shows invalid_display_name from the server', async () => {
    mockFetch(base({ 'PATCH /api/v1/me': problem(400, 'invalid_display_name') }));
    const { wrapper } = await renderAt('/account');
    await wrapper.find('#display-name').setValue('Ali');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').text()).toBe(
      'Enter a display name of 1 to 80 characters.',
    );
  });

  it('uploads a picture', async () => {
    const { calls } = mockFetch(
      base({
        'PUT /api/v1/me/picture': { status: 200, body: { ...ALICE, pictureVersion: 'v2' } },
      }),
    );
    const { wrapper } = await renderAt('/account');
    expect(button(wrapper, 'Remove picture')).toBeUndefined();
    await pickFile(wrapper, new File(['png-bytes'], 'a.png', { type: 'image/png' }));
    const put = calls.find((c) => c.method === 'PUT');
    expect(put?.path).toBe('/api/v1/me/picture');
    expect(put?.headers.get('Content-Type')).toBe('image/png');
    expect(button(wrapper, 'Remove picture')).toBeDefined();
  });

  it('rejects a file over 5 MB without uploading', async () => {
    const { calls } = mockFetch(base());
    const { wrapper } = await renderAt('/account');
    const big = new File([new Uint8Array(5 * 1024 * 1024 + 1)], 'big.png', { type: 'image/png' });
    await pickFile(wrapper, big);
    expect(calls.some((c) => c.method === 'PUT')).toBe(false);
    expect(wrapper.find('[role="alert"]').text()).toBe(
      'This picture is larger than 5 MB. Choose a smaller one.',
    );
  });

  it('maps a bare 413 and an unsupported type to their messages', async () => {
    let status = 413;
    mockFetch(
      base({
        'PUT /api/v1/me/picture': () =>
          status === 413 ? { status: 413 } : problem(415, 'unsupported_picture_type'),
      }),
    );
    const { wrapper } = await renderAt('/account');
    await pickFile(wrapper, new File(['x'], 'a.png', { type: 'image/png' }));
    expect(wrapper.find('[role="alert"]').text()).toContain('larger than 5 MB');
    status = 415;
    await pickFile(wrapper, new File(['x'], 'a.bmp', { type: 'image/bmp' }));
    expect(wrapper.find('[role="alert"]').text()).toContain('PNG, JPEG, GIF or WebP');
  });

  it('Remove picture appears only with a picture and removes it', async () => {
    const { calls } = mockFetch(
      base({
        'POST /api/v1/auth/refresh': session(withPicture),
        'GET /api/v1/me': me([team()], [], withPicture),
        'DELETE /api/v1/me/picture': { status: 200, body: ALICE },
      }),
    );
    const { wrapper } = await renderAt('/account');
    const remove = button(wrapper, 'Remove picture');
    expect(remove).toBeDefined();
    await remove?.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.method === 'DELETE' && c.path === '/api/v1/me/picture')).toBe(true);
    expect(button(wrapper, 'Remove picture')).toBeUndefined();
  });

  it('shows the SSO email hint for sso users and the local hint otherwise', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team()], [], ALICE, { signInMethod: 'sso' }) }));
    const sso = await renderAt('/account');
    expect(sso.wrapper.text()).toContain(
      "Comes from your organization's sign-in and can't be changed here. Invitations are matched to this address.",
    );
    expect(sso.wrapper.text()).toContain('Signed in with single sign-on');
    expect(sso.wrapper.text()).toContain('Member since 29 Sep 2026');
    expect(sso.wrapper.text()).toContain(ALICE.email);
    sso.wrapper.unmount();

    mockFetch(base());
    const local = await renderAt('/account');
    expect(local.wrapper.text()).toContain('Invitations are matched to this address.');
    expect(local.wrapper.text()).not.toContain("Comes from your organization's");
    expect(local.wrapper.text()).toContain('Signed in with username and password');
  });
});

const mobile = () =>
  team({ id: OTHER_TEAM_ID, name: 'Mobile Squad', role: 'member', memberCount: 4 });

function rows(wrapper: Awaited<ReturnType<typeof renderAt>>['wrapper']) {
  return wrapper.findAll('[data-testid="team-row"]');
}

describe('account page: teams', () => {
  beforeEach(() => localStorage.clear());

  it('lists teams with role and member count', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team({ memberCount: 6 }), mobile()]) }));
    const { wrapper } = await renderAt('/account');
    expect(wrapper.text()).toContain('Your teams · 2');
    expect(wrapper.text()).toContain('Last admins need to promote someone else before leaving.');
    const [first, second] = rows(wrapper);
    expect(first?.text()).toContain('Platform Team');
    expect(first?.text()).toContain('Current');
    expect(first?.text()).toContain('6 members');
    expect(first?.text()).toContain('Admin');
    expect(second?.text()).toContain('Mobile Squad');
    expect(second?.text()).not.toContain('Current');
    expect(second?.text()).toContain('4 members');
    expect(second?.text()).toContain('Member');
  });

  it("Open goes to the team's meetings", async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team(), mobile()]) }));
    const { wrapper, router } = await renderAt('/account');
    await rows(wrapper)[1]?.find('a').trigger('click');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`),
    );
  });

  it('Leave removes you and stays on the account page', async () => {
    let left = false;
    const { calls } = mockFetch(
      base({
        'GET /api/v1/me': () => me(left ? [team()] : [team(), mobile()]),
        [`DELETE /api/v1/teams/${OTHER_TEAM_ID}/members/${ALICE.id}`]: () => {
          left = true;
          return { status: 204 };
        },
      }),
    );
    const { wrapper, router } = await renderAt('/account');
    await button(rows(wrapper)[1] as never, 'Leave')?.trigger('click');
    expect(wrapper.text()).toContain('Leave Mobile Squad?');
    await button(wrapper, 'Leave team')?.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.method === 'DELETE')).toBe(true);
    expect(router.currentRoute.value.fullPath).toBe('/account');
    expect(rows(wrapper)).toHaveLength(1);
    expect(wrapper.text()).toContain('Your teams · 1');
  });

  it('TeamsSection shows last_admin', async () => {
    mockFetch(
      base({
        [`DELETE /api/v1/teams/${TEAM_ID}/members/${ALICE.id}`]: problem(409, 'last_admin'),
      }),
    );
    const { wrapper } = await renderAt('/account');
    await button(rows(wrapper)[0] as never, 'Leave')?.trigger('click');
    await button(wrapper, 'Leave team')?.trigger('click');
    await vi.waitFor(() => expect(wrapper.text()).toContain('Every team needs at least one admin'));
  });

  it('TeamsSection warns a sole member', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team({ memberCount: 1 })]) }));
    const { wrapper } = await renderAt('/account');
    await button(rows(wrapper)[0] as never, 'Leave')?.trigger('click');
    expect(wrapper.text()).toContain('Leave and delete Platform Team?');
  });

  it('Create team opens the create dialog', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    await button(wrapper, 'Create team')?.trigger('click');
    await vi.waitFor(() => expect(wrapper.find('#new-team').exists()).toBe(true));
  });
});

describe('account page: invitations', () => {
  beforeEach(() => localStorage.clear());

  it('lists invitations with who invited you and when it expires', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([team()], [pendingInvitation()]) }));
    const { wrapper } = await renderAt('/account');
    const panel = wrapper.find('[aria-labelledby="invitations-title"]');
    expect(panel.text()).toContain('Pending invitations · 1');
    expect(panel.text()).toContain(`Sent to ${ALICE.email}`);
    expect(panel.text()).toContain('Mobile Squad');
    expect(panel.text()).toContain('Invited by Bob Bianchi · expires in 6 days');
  });

  it('accepting an invitation opens the team', async () => {
    let accepted = false;
    mockFetch(
      base({
        'GET /api/v1/me': () =>
          accepted ? me([team(), mobile()]) : me([team()], [pendingInvitation()]),
        'POST /api/v1/invitations/tok-mobile/accept': () => {
          accepted = true;
          return { status: 200, body: { id: OTHER_TEAM_ID, name: 'Mobile Squad' } };
        },
      }),
    );
    const { wrapper, router } = await renderAt('/account');
    await button(wrapper, 'Accept')?.trigger('click');
    await vi.waitFor(() =>
      expect(router.currentRoute.value.fullPath).toBe(`/teams/${OTHER_TEAM_ID}/meetings`),
    );
  });

  it('declining removes the row', async () => {
    let declined = false;
    const { calls } = mockFetch(
      base({
        'GET /api/v1/me': () => me([team()], declined ? [] : [pendingInvitation()]),
        'POST /api/v1/invitations/tok-mobile/decline': () => {
          declined = true;
          return { status: 204 };
        },
      }),
    );
    const { wrapper } = await renderAt('/account');
    await button(wrapper, 'Decline')?.trigger('click');
    await flushPromises();
    expect(calls.some((c) => c.path === '/api/v1/invitations/tok-mobile/decline')).toBe(true);
    expect(wrapper.text()).not.toContain('Pending invitations');
  });

  it('shows why an invitation could not be accepted', async () => {
    mockFetch(
      base({
        'GET /api/v1/me': me([team()], [pendingInvitation()]),
        'POST /api/v1/invitations/tok-mobile/accept': problem(410, 'invitation_revoked'),
      }),
    );
    const { wrapper } = await renderAt('/account');
    await button(wrapper, 'Accept')?.trigger('click');
    await flushPromises();
    expect(wrapper.find('[aria-labelledby="invitations-title"]').text()).toContain(
      'This invitation was revoked.',
    );
  });

  it('the invitations panel is hidden when there are none', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    expect(wrapper.find('[aria-labelledby="invitations-title"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain('Pending invitations');
  });
});

describe('account page: preferences and log out', () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => {
    // Back to "system": clears `data-theme` and the stored preference.
    useThemePreference().setTheme('system');
  });

  it('switches theme', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    const dark = wrapper.findAll('[role="group"] button').find((b) => b.text() === 'Dark');
    await dark?.trigger('click');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(dark?.attributes('aria-pressed')).toBe('true');
  });

  it('keeps the user menu theme control in sync', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    await wrapper
      .findAll('main [role="group"] button')
      .find((b) => b.text() === 'Dark')
      ?.trigger('click');
    const menuDark = wrapper.findAll('#user-menu button').find((b) => b.text() === 'Dark');
    expect(menuDark?.attributes('aria-pressed')).toBe('true');
  });

  const twoTeams = () =>
    base({
      'GET /api/v1/me': me([
        team(),
        team({ id: OTHER_TEAM_ID, name: 'Mobile Squad', role: 'member', memberCount: 4 }),
      ]),
    });
  const field = (wrapper: VueWrapper) => wrapper.get('[data-testid="default-team"]');
  const fieldTrigger = (wrapper: VueWrapper) =>
    field(wrapper).get('button[aria-haspopup="listbox"]');
  const teamOption = (wrapper: VueWrapper, label: string) =>
    field(wrapper)
      .findAll('[role="option"]')
      .find((o) => o.get('.option__label').text() === label)!;

  it('pins a default team, and Last used unpins it', async () => {
    mockFetch(twoTeams());
    const { wrapper } = await renderAt('/account');
    expect(wrapper.text()).toContain('The team Retro opens after you sign in.');
    expect(fieldTrigger(wrapper).text()).toBe('Last used');

    await fieldTrigger(wrapper).trigger('click');
    const rows = field(wrapper).findAll('[role="option"]');
    expect(rows.map((r) => r.get('.option__label').text())).toEqual([
      'Last used',
      'Platform Team',
      'Mobile Squad',
    ]);
    expect(rows.map((r) => r.find('.option__hint').text())).toEqual([
      'Opens whichever team you used last',
      'Admin · 2 members',
      'Member · 4 members',
    ]);
    expect(teamOption(wrapper, 'Last used').attributes('aria-selected')).toBe('true');

    await teamOption(wrapper, 'Mobile Squad').trigger('click');
    expect(localStorage.getItem(`retro.defaultTeam.${ALICE.id}`)).toBe(OTHER_TEAM_ID);
    expect(fieldTrigger(wrapper).text()).toContain('Mobile Squad');
    expect(fieldTrigger(wrapper).attributes('aria-expanded')).toBe('false');

    await fieldTrigger(wrapper).trigger('click');
    await teamOption(wrapper, 'Last used').trigger('click');
    expect(localStorage.getItem(`retro.defaultTeam.${ALICE.id}`)).toBeNull();
    expect(fieldTrigger(wrapper).text()).toBe('Last used');
  });

  it('keeps the Current tag on the last used team when pinning another', async () => {
    mockFetch(twoTeams());
    const { wrapper } = await renderAt('/account');
    const currentRow = () =>
      wrapper.findAll('[data-testid="team-row"]').find((r) => r.text().includes('Current'));
    expect(currentRow()?.text()).toContain('Platform Team');
    await fieldTrigger(wrapper).trigger('click');
    await teamOption(wrapper, 'Mobile Squad').trigger('click');
    expect(currentRow()?.text()).toContain('Platform Team');
    expect(wrapper.findAll('.tag--current')).toHaveLength(1);
  });

  it('stays mounted when the default team changes', async () => {
    const { calls } = mockFetch(twoTeams());
    const { wrapper } = await renderAt('/account');
    const meCalls = () => calls.filter((c) => c.method === 'GET' && c.path === '/api/v1/me');
    // One from the teams store (route guard), one from the page itself.
    const before = meCalls().length;
    expect(before).toBe(2);
    await wrapper.find('#display-name').setValue('Ali');
    const trigger = fieldTrigger(wrapper);
    await trigger.trigger('click');
    await teamOption(wrapper, 'Mobile Squad').trigger('click');
    await flushPromises();
    expect((wrapper.find('#display-name').element as HTMLInputElement).value).toBe('Ali');
    expect(fieldTrigger(wrapper).element).toBe(trigger.element);
    expect(meCalls()).toHaveLength(before);
  });

  it('the language field offers English only', async () => {
    mockFetch(base());
    const { wrapper } = await renderAt('/account');
    const language = wrapper.get('button[aria-labelledby="language-label language-value"]');
    expect(language.text()).toBe('English');
    await language.trigger('click');
    const options = wrapper.findAll('[role="listbox"][aria-label="Language"] [role="option"]');
    expect(options.map((o) => o.text())).toEqual(['English']);
  });

  it('hides the default team control without teams', async () => {
    mockFetch(base({ 'GET /api/v1/me': me([]) }));
    const { wrapper } = await renderAt('/account');
    expect(wrapper.find('[data-testid="default-team"]').exists()).toBe(false);
  });

  it('logs out', async () => {
    const { calls } = mockFetch(
      base({
        'POST /api/v1/auth/logout': { status: 204 },
        'GET /api/v1/auth/config': {
          status: 200,
          body: {
            localLogin: true,
            sso: { enabled: false, buttonText: '' },
            passwordResetUrl: null,
          },
        },
      }),
    );
    const { wrapper, router } = await renderAt('/account');
    expect(wrapper.find('[aria-label="Session"]').text()).toContain(
      'Ends your session on this device.',
    );
    await wrapper.find('[aria-label="Session"] button').trigger('click');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('login'));
    expect(calls.some((c) => c.method === 'POST' && c.path === '/api/v1/auth/logout')).toBe(true);
  });

  it('still lands on the login page when the logout request fails', async () => {
    mockFetch(
      base({
        'POST /api/v1/auth/logout': { status: 500 },
        'GET /api/v1/auth/config': {
          status: 200,
          body: {
            localLogin: true,
            sso: { enabled: false, buttonText: '' },
            passwordResetUrl: null,
          },
        },
      }),
    );
    const { wrapper, router } = await renderAt('/account');
    await wrapper.find('[aria-label="Session"] button').trigger('click');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('login'));
  });
});
