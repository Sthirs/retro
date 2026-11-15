import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import UserAvatar from '@/components/UserAvatar.vue';
import { clearImageCache } from '@/lib/images';

import { ALICE, mockFetch, problem } from './helpers';

const PICTURE_PATH = `/api/v1/users/${ALICE.id}/picture`;
const png = () => ({
  status: 200,
  body: new Blob([new Uint8Array([137, 80, 78, 71])], { type: 'image/png' }),
});

function avatar(pictureVersion?: string | null) {
  return mount(UserAvatar, {
    props: { userId: ALICE.id, name: ALICE.displayName, pictureVersion },
  });
}

describe('UserAvatar', () => {
  let createObjectURL: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    createObjectURL = vi.fn(() => 'blob:x');
    // happy-dom has no object URLs; patch the real URL class so `new URL()` keeps working.
    Object.assign(URL, { createObjectURL, revokeObjectURL: vi.fn() });
  });

  afterEach(async () => {
    clearImageCache();
    await flushPromises();
    Reflect.deleteProperty(URL, 'createObjectURL');
    Reflect.deleteProperty(URL, 'revokeObjectURL');
    vi.unstubAllGlobals();
  });

  it('UserAvatar shows initials without a picture', async () => {
    const { calls } = mockFetch({});
    const wrapper = avatar(null);
    await flushPromises();
    expect(wrapper.text()).toBe('AR');
    expect(wrapper.find('img').exists()).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it('UserAvatar shows the picture once loaded', async () => {
    mockFetch({ [`GET ${PICTURE_PATH}`]: png });
    const wrapper = avatar('a');
    expect(wrapper.text()).toBe('AR');
    await flushPromises();
    const img = wrapper.find('img[src="blob:x"]');
    expect(img.exists()).toBe(true);
    expect(img.attributes('alt')).toBe('');
    expect(wrapper.text()).toBe('');
  });

  it('UserAvatar falls back to initials when the picture fails', async () => {
    mockFetch({ [`GET ${PICTURE_PATH}`]: problem(404, 'not_found') });
    const wrapper = avatar('a');
    await flushPromises();
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.text()).toBe('AR');
  });

  it('UserAvatar reloads when pictureVersion changes', async () => {
    const { calls } = mockFetch({ [`GET ${PICTURE_PATH}`]: png });
    const wrapper = avatar('a');
    await flushPromises();
    createObjectURL.mockReturnValue('blob:y');
    await wrapper.setProps({ pictureVersion: 'b' });
    await flushPromises();
    expect(calls.map((c) => c.path)).toEqual([`${PICTURE_PATH}?v=a`, `${PICTURE_PATH}?v=b`]);
    expect(wrapper.find('img[src="blob:y"]').exists()).toBe(true);
  });
});
