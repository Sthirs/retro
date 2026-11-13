import { describe, expect, it } from 'vitest';

import { ssoUrl } from '@/auth/config';
import { AVATAR_COLORS, avatarColorIndex, initials } from '@/lib/avatar';
import { safeRedirect } from '@/lib/redirect';
import { applyThemePreference, readThemePreference, resolveTheme } from '@/lib/theme';
import { relativeFromNow } from '@/lib/time';

describe('theme', () => {
  it('resolves system preference', () => {
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
  });

  it('applies and persists an explicit choice', () => {
    const root = document.createElement('html');
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k),
    };
    applyThemePreference('dark', root, storage);
    expect(root.getAttribute('data-theme')).toBe('dark');
    expect(readThemePreference(storage)).toBe('dark');

    applyThemePreference('system', root, storage);
    expect(root.hasAttribute('data-theme')).toBe(false);
    expect(readThemePreference(storage)).toBe('system');
  });

  it('falls back to system when storage throws', () => {
    const broken = {
      getItem: () => {
        throw new Error('denied');
      },
    };
    expect(readThemePreference(broken)).toBe('system');
  });
});

describe('avatar', () => {
  it('derives initials', () => {
    expect(initials('Alice Rossi')).toBe('AR');
    expect(initials('  bob  ')).toBe('B');
    expect(initials('Anna Maria Bianchi')).toBe('AB');
    expect(initials('')).toBe('?');
  });

  it('picks a stable colour in range', () => {
    const id = '0190a4e0-0000-7000-8000-000000000001';
    expect(avatarColorIndex(id)).toBe(avatarColorIndex(id));
    for (const sample of ['a', 'b', 'c', id]) {
      const index = avatarColorIndex(sample);
      expect(index).toBeGreaterThanOrEqual(1);
      expect(index).toBeLessThanOrEqual(AVATAR_COLORS);
    }
  });
});

describe('redirects', () => {
  it('accepts only local paths', () => {
    expect(safeRedirect('/teams/1')).toBe('/teams/1');
    expect(safeRedirect('//evil.example')).toBe('/');
    expect(safeRedirect('https://evil.example')).toBe('/');
    expect(safeRedirect('/\\evil.example')).toBe('/');
    expect(safeRedirect(['/a'])).toBe('/');
    expect(safeRedirect(undefined)).toBe('/');
  });

  it('builds the single sign-on URL', () => {
    expect(ssoUrl('/', false)).toBe('/api/v1/auth/oidc?redirect=%2F');
    expect(ssoUrl('/x', true)).toBe('/api/v1/auth/oidc?redirect=%2Fx&remember=true');
  });
});

describe('relativeFromNow', () => {
  const now = Date.parse('2026-11-13T12:00:00Z');
  const at = (ms: number) => new Date(now + ms).toISOString();

  it('says days, hours or minutes from now', () => {
    expect(relativeFromNow(at(6 * 24 * 3600e3), 'en', now)).toBe('in 6 days');
    expect(relativeFromNow(at(24 * 3600e3), 'en', now)).toBe('tomorrow');
    expect(relativeFromNow(at(3 * 3600e3), 'en', now)).toBe('in 3 hours');
    expect(relativeFromNow(at(10 * 60e3), 'en', now)).toBe('in 10 minutes');
  });
});
