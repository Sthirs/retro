import { flushPromises } from '@vue/test-utils';
import { beforeEach, describe, expect, it } from 'vitest';

import { ALICE, me, mockFetch, problem, renderAt, session, team, type FakeRoute } from './helpers';

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
