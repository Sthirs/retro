import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

import { realtime } from '@/api/realtime';
import { useMeetingStore } from '@/stores/meeting';
import { hasText, useReflectDraftsStore } from '@/stores/reflectDrafts';
import { useSessionStore } from '@/stores/session';

import { ALICE, MEETING_ID, meetingState, mockFetch } from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;
const KEY = `retro.reflectDrafts.${MEETING_ID}.${ALICE.id}`;

async function setup() {
  const state = meetingState({ phase: 'reflect' });
  mockFetch({
    [`POST ${BASE}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
  });
  const store = useMeetingStore();
  useSessionStore().setUser({ ...ALICE, pictureVersion: null });
  await nextTick();
  await store.open(MEETING_ID);
  const drafts = useReflectDraftsStore();
  await nextTick();
  return { store, drafts };
}

describe('reflect drafts', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    localStorage.clear();
  });
  afterEach(() => vi.restoreAllMocks());

  it('tells markup with text from empty markup', () => {
    expect(hasText('<p>Hi</p>')).toBe(true);
    expect(hasText('<p></p>')).toBe(false);
    expect(hasText('<p>&nbsp; </p>')).toBe(false);
    expect(hasText('<p><img src="x"></p>')).toBe(false);
    expect(hasText('<<b>p>a')).toBe(true);
  });

  it('keeps a draft another tab saved in another column', async () => {
    const { drafts } = await setup();
    drafts.setDraft('plus', '<p>Plus</p>');
    // Another tab, loaded before the Plus draft was saved, saves Delta.
    localStorage.setItem(
      KEY,
      JSON.stringify({ ...JSON.parse(localStorage.getItem(KEY)!), delta: '<p>Delta</p>' }),
    );
    drafts.setDraft('ideas', '<p>Idea</p>');
    drafts.clearDraft('ideas');
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({
      plus: '<p>Plus</p>',
      delta: '<p>Delta</p>',
    });
  });

  it('keeps drafts in this browser, per meeting and user', async () => {
    const { drafts } = await setup();
    drafts.setDraft('delta', '<p>Flaky tests</p>');
    drafts.setDraft('plus', '<p></p>');
    expect(drafts.unsent).toBe(1);
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ delta: '<p>Flaky tests</p>' });

    // A reload reads them back.
    setActivePinia(createPinia());
    const again = await setup();
    expect(again.drafts.draft('delta')).toBe('<p>Flaky tests</p>');
    expect(again.drafts.unsent).toBe(1);

    again.drafts.clearDraft('delta');
    expect(localStorage.getItem(KEY)).toBeNull();
  });

  it('announces the open editor as what this tab is writing', async () => {
    const setWriting = vi.spyOn(realtime, 'setWriting');
    const { drafts } = await setup();
    drafts.startNew('ideas');
    await nextTick();
    expect(setWriting).toHaveBeenLastCalledWith(channel, { column: 'ideas', mode: 'new' });
    drafts.startEdit('r1', 'plus');
    await nextTick();
    expect(setWriting).toHaveBeenLastCalledWith(channel, { column: 'plus', mode: 'edit' });
    drafts.close();
    await nextTick();
    expect(setWriting).toHaveBeenLastCalledWith(channel, null);
  });

  it('discards the drafts when the meeting leaves Reflect, and says so', async () => {
    const setWriting = vi.spyOn(realtime, 'setWriting');
    const { store, drafts } = await setup();
    drafts.setDraft('thanks', '<p>Thanks Bob</p>');
    drafts.startNew('thanks');
    await nextTick();
    store.apply({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'group' } });
    await nextTick();
    expect(drafts.unsent).toBe(0);
    expect(drafts.open).toBeNull();
    expect(localStorage.getItem(KEY)).toBeNull();
    expect(store.notice).toEqual({ kind: 'draftsDiscarded', count: 1 });
    expect(setWriting).toHaveBeenLastCalledWith(channel, null);
  });

  it('says nothing when there was nothing to discard', async () => {
    const { store } = await setup();
    store.apply({ type: 'phase.changed', channel, seq: 3, payload: { phase: 'group' } });
    await nextTick();
    expect(store.notice).toBeNull();
  });
});
