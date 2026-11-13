import { flushPromises, mount } from '@vue/test-utils';
import type { Plugin } from 'vue';
import { describe, expect, it, vi } from 'vitest';

import App from '@/App.vue';

import { ALICE, authConfig, mockFetch, session, setup, unauthorized } from './helpers';

async function renderAt(path: string) {
  const { router, plugins } = setup();
  await router.push(path);
  await router.isReady();
  const wrapper = mount(App, { global: { plugins: plugins as Plugin[] }, attachTo: document.body });
  await flushPromises();
  return { wrapper, router };
}

describe('SignInView', () => {
  it('renders the design copy and both sign-in methods', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login');

    expect(wrapper.find('h1').text()).toBe('Sign in to Retro');
    expect(wrapper.text()).toContain('Run better retrospectives with your team.');
    expect(wrapper.find('label[for="si-user"]').text()).toBe('Username or email');
    expect(wrapper.find('label[for="si-pass"]').text()).toBe('Password');
    expect(wrapper.text()).toContain('Keep me signed in');
    expect(wrapper.find('button[type="submit"]').text()).toBe('Sign in');
    expect(wrapper.text()).toContain('Continue with single sign-on');
    expect(wrapper.text()).toContain(
      "Single sign-on takes you to your organization's sign-in page and brings you straight back.",
    );
    // No reset URL configured: no "Forgot password?" link.
    expect(wrapper.text()).not.toContain('Forgot password?');
  });

  it('shows "Forgot password?" when a reset URL is configured', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig({ passwordResetUrl: 'https://id.example.com/reset' }),
    });
    const { wrapper } = await renderAt('/login');
    const link = wrapper.find('a.forgot');
    expect(link.text()).toBe('Forgot password?');
    expect(link.attributes('href')).toBe('https://id.example.com/reset');
  });

  it('toggles password visibility', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login');
    const input = wrapper.find('#si-pass');
    const toggle = wrapper.find('button[aria-controls="si-pass"]');
    expect(input.attributes('type')).toBe('password');
    expect(toggle.attributes('aria-label')).toBe('Show password');

    await toggle.trigger('click');
    expect(input.attributes('type')).toBe('text');
    expect(toggle.attributes('aria-label')).toBe('Hide password');
    expect(toggle.attributes('aria-pressed')).toBe('true');
  });

  it('signs in with "keep me signed in" and goes to the redirect target', async () => {
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
      'POST /api/v1/auth/login': session(),
    });
    const { wrapper, router } = await renderAt('/login?redirect=/somewhere');

    await wrapper.find('#si-user').setValue('alice');
    await wrapper.find('#si-pass').setValue('password');
    await wrapper.find('input[type="checkbox"]').setValue(true);
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    const login = calls.find((c) => c.path === '/api/v1/auth/login');
    expect(login?.body).toEqual({ login: 'alice', password: 'password', remember: true });
    // /somewhere is unknown and falls back to home, which needs the session.
    await vi.waitFor(() => expect(router.currentRoute.value.path).toBe('/'));
    await flushPromises();
    await vi.waitFor(() =>
      expect(wrapper.find('[data-testid="current-user"]').text()).toBe(ALICE.displayName),
    );
  });

  it('shows an error on wrong credentials and clears the password', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
      'POST /api/v1/auth/login': {
        status: 401,
        body: {
          type: 'about:blank',
          title: 'Unauthorized',
          status: 401,
          code: 'invalid_credentials',
        },
      },
    });
    const { wrapper, router } = await renderAt('/login');

    await wrapper.find('#si-user').setValue('alice');
    await wrapper.find('#si-pass').setValue('wrong');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(wrapper.find('[role="alert"]').text()).toBe('Incorrect username or password.');
    expect((wrapper.find('#si-pass').element as HTMLInputElement).value).toBe('');
    expect(wrapper.find('#si-user').attributes('aria-invalid')).toBe('true');
    expect(router.currentRoute.value.path).toBe('/login');
  });

  it('asks for both fields before calling the API', async () => {
    const { calls } = mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').text()).toContain('Enter your username or email');
    expect(calls.some((c) => c.path === '/api/v1/auth/login')).toBe(false);
  });

  it('reports rate limiting', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
      'POST /api/v1/auth/login': { status: 429, body: { code: 'too_many_requests' } },
    });
    const { wrapper } = await renderAt('/login');
    await wrapper.find('#si-user').setValue('alice');
    await wrapper.find('#si-pass').setValue('x');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').text()).toContain('Too many sign-in attempts');
  });

  it('links single sign-on with the redirect and remember flag', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login?redirect=/teams/1');
    const sso = () => wrapper.find('a.btn--secondary');
    expect(sso().attributes('href')).toBe('/api/v1/auth/oidc?redirect=%2Fteams%2F1');
    await wrapper.find('input[type="checkbox"]').setValue(true);
    expect(sso().attributes('href')).toBe('/api/v1/auth/oidc?redirect=%2Fteams%2F1&remember=true');
  });

  it('hides single sign-on when disabled', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig({ sso: { enabled: false, buttonText: '' } }),
    });
    const { wrapper } = await renderAt('/login');
    expect(wrapper.find('form').exists()).toBe(true);
    expect(wrapper.text()).not.toContain('single sign-on');
    expect(wrapper.find('.divider').exists()).toBe(false);
  });

  it('hides the password form when local sign-in is disabled', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig({ localLogin: false }),
    });
    const { wrapper } = await renderAt('/login');
    expect(wrapper.find('form').exists()).toBe(false);
    expect(wrapper.find('.divider').exists()).toBe(false);
    expect(wrapper.text()).toContain('Continue with single sign-on');
  });

  it('explains a failed single sign-on', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login?error=sso_not_registered');
    expect(wrapper.find('[role="alert"]').text()).toBe(
      'Your organization account is not registered in Retro.',
    );
  });

  it('ignores unknown error codes in the URL', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': unauthorized,
      'GET /api/v1/auth/config': authConfig(),
    });
    const { wrapper } = await renderAt('/login?error=<script>');
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });
});
