import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';
import { nextTick } from 'vue';

import { CLIENT_ID } from '@/api/clientId';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

import { ALICE, BOB, MEETING_ID, meetingState, mockFetch, problem } from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;

const card = (id: string, column: string, position: number, overrides = {}) => ({
  id,
  column,
  content: `<p>${id}</p>`,
  mine: false,
  groupId: null as string | null,
  position,
  version: 0,
  createdAt: '2026-11-10T17:03:00Z',
  updatedAt: '2026-11-10T17:03:00Z',
  ...overrides,
});

const grouping = (overrides: Record<string, unknown> = {}) =>
  meetingState({
    phase: 'group',
    reflections: [
      card('p1', 'plus', 0),
      card('p2', 'plus', 1),
      card('d1', 'delta', 0),
      card('d2', 'delta', 1),
      card('i1', 'ideas', 0),
    ],
    ...overrides,
  });

async function opened(state = grouping(), routes = {}) {
  const fetch = mockFetch({
    [`POST ${BASE}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
    ...routes,
  });
  const store = useMeetingStore();
  useSessionStore().setUser({ ...ALICE, pictureVersion: null });
  await nextTick();
  await store.open(MEETING_ID);
  return { store, ...fetch };
}

const ids = (store: ReturnType<typeof useMeetingStore>, column: string) =>
  (store.columnItems.get(column as never) ?? []).map((item) =>
    item.kind === 'card' ? item.card.id : `group:${item.cards.map((c) => c.id).join('+')}`,
  );

describe('meeting store in Group', () => {
  beforeEach(() => setActivePinia(createPinia()));

  it('lays out each column: cards and groups at their places', async () => {
    const { store } = await opened(
      grouping({
        reflections: [
          card('p1', 'plus', 0),
          card('p2', 'plus', 1, { groupId: 'g1' }),
          card('d1', 'delta', 0),
          card('d2', 'delta', 1, { groupId: 'g1' }),
          card('i1', 'ideas', 0, { groupId: 'g1' }),
        ],
        groups: [{ id: 'g1', column: 'plus', position: 1, name: 'Shipping' }],
      }),
    );
    expect(ids(store, 'plus')).toEqual(['p1', 'group:p2+d2+i1']);
    expect(ids(store, 'delta')).toEqual(['d1']);
    expect(ids(store, 'ideas')).toEqual([]);
  });

  it('applies other people’s group changes and skips its own echo', async () => {
    const { store } = await opened();
    store.apply({
      type: 'group.created',
      channel,
      seq: 3,
      payload: {
        group: { id: 'g1', column: 'delta', position: 1, name: '' },
        reflections: [
          { id: 'd2', groupId: 'g1', version: 1 },
          { id: 'i1', groupId: 'g1', version: 1 },
        ],
        clientId: 'another-tab',
      },
    });
    expect(ids(store, 'delta')).toEqual(['d1', 'group:d2+i1']);
    expect(store.meeting?.reflections.find((r) => r.id === 'i1')?.version).toBe(1);

    store.apply({
      type: 'group.updated',
      channel,
      seq: 4,
      payload: {
        group: { id: 'g1', column: 'delta', position: 1, name: 'CI' },
        reflections: [],
        clientId: null,
      },
    });
    expect(store.meeting?.groups[0]?.name).toBe('CI');

    // Our own echo: the answer to our request already brought it.
    store.apply({
      type: 'group.updated',
      channel,
      seq: 5,
      payload: {
        group: { id: 'g1', column: 'delta', position: 1, name: 'Echo' },
        reflections: [{ id: 'p1', groupId: 'g1', version: 1 }],
        clientId: CLIENT_ID,
      },
    });
    expect(store.meeting?.groups[0]?.name).toBe('CI');
    expect(store.meeting?.seq).toBe(5);

    store.apply({
      type: 'group.deleted',
      channel,
      seq: 6,
      payload: {
        group: { id: 'g1' },
        reflections: [
          { id: 'd2', groupId: null, version: 2 },
          { id: 'i1', groupId: null, version: 2 },
        ],
        clientId: null,
      },
    });
    expect(store.meeting?.groups).toEqual([]);
    expect(ids(store, 'ideas')).toEqual(['i1']);
  });

  it('knows who else drags which card', async () => {
    const { store } = await opened();
    store.apply({
      type: 'card.dragging',
      channel,
      seq: null,
      payload: {
        drags: [
          { reflectionId: 'p1', userId: BOB.id },
          { reflectionId: 'd1', userId: ALICE.id },
        ],
      },
    });
    expect(store.draggers.get('p1')?.id).toBe(BOB.id);
    // Our own drag shows no badge.
    expect(store.draggers.has('d1')).toBe(false);
  });

  it('groups optimistically and takes the server’s answer', async () => {
    const answer = grouping({
      reflections: [
        card('p1', 'plus', 0),
        card('p2', 'plus', 1),
        card('d1', 'delta', 0),
        card('d2', 'delta', 1, { groupId: 'g9', version: 1 }),
        card('i1', 'ideas', 0, { groupId: 'g9', version: 1 }),
      ],
      groups: [{ id: 'g9', column: 'delta', position: 1, name: '' }],
      seq: 3,
    });
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const { store, calls } = await opened(grouping(), {
      [`POST ${BASE}/groups`]: async () => {
        await gate;
        return { status: 200, body: answer };
      },
    });
    const done = store.dropOnCard('d2', 'i1');
    // At once: a group at d2's place.
    expect(ids(store, 'delta')).toEqual(['d1', 'group:d2+i1']);
    expect(ids(store, 'ideas')).toEqual([]);
    release();
    expect(await done).toBe(true);
    expect(calls.at(-1)?.body).toEqual({ targetId: 'd2', reflectionId: 'i1' });
    expect(store.meeting?.groups.map((g) => g.id)).toEqual(['g9']);
  });

  it('dissolves a pair optimistically and undoes a refused move', async () => {
    const { store } = await opened(
      grouping({
        reflections: [
          card('p1', 'plus', 0),
          card('p2', 'plus', 1),
          card('d1', 'delta', 0, { groupId: 'g1' }),
          card('d2', 'delta', 1, { groupId: 'g1' }),
          card('i1', 'ideas', 0),
        ],
        groups: [{ id: 'g1', column: 'delta', position: 0, name: 'Pair' }],
      }),
      { 'PUT /api/v1/reflections/d1/group': problem(409, 'wrong_phase') },
    );
    const done = store.moveToGroup('d1', null);
    expect(store.meeting?.groups).toEqual([]);
    expect(ids(store, 'delta')).toEqual(['d1', 'd2']);
    expect(await done).toBe(false);
    expect(store.actionError).toBe('wrong_phase');
    expect(ids(store, 'delta')).toEqual(['group:d1+d2']);
  });

  it('renames a group with the trimmed name', async () => {
    const { store, calls } = await opened(
      grouping({
        reflections: [
          card('d1', 'delta', 0, { groupId: 'g1' }),
          card('d2', 'delta', 1, { groupId: 'g1' }),
        ],
        groups: [{ id: 'g1', column: 'delta', position: 0, name: '' }],
      }),
      { 'PATCH /api/v1/groups/g1': { status: 200, body: grouping() } },
    );
    const done = store.renameGroup('g1', '  CI  ');
    expect(store.meeting?.groups[0]?.name).toBe('CI');
    await done;
    expect(calls.at(-1)?.body).toEqual({ name: '  CI  ' });
  });
});
