import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, it } from 'vitest';

import { useMeetingsStore } from '@/stores/meetings';

import { ALICE, BOB, meetingSummary, MEETING_ID, mockFetch, participant, TEAM_ID } from './helpers';

const LIST = `GET /api/v1/teams/${TEAM_ID}/meetings`;

describe('meetings store', () => {
  beforeEach(() => setActivePinia(createPinia()));

  async function loaded() {
    const fetch = mockFetch({
      [LIST]: {
        status: 200,
        body: [
          meetingSummary({
            participants: [
              participant(ALICE, { online: true }),
              participant(BOB, { online: false }),
            ],
          }),
        ],
      },
    });
    const store = useMeetingsStore();
    await store.load(TEAM_ID);
    return { store, ...fetch };
  }

  it("patches a participant's presence in place, without a request", async () => {
    const { store, calls } = await loaded();
    store.applyPresence(MEETING_ID, BOB.id, true);
    store.applyPresence(MEETING_ID, ALICE.id, false);
    const people = store.meetings[0]!.participants;
    expect(people.map((p) => [p.id, p.online])).toEqual([
      [ALICE.id, false],
      [BOB.id, true],
    ]);
    expect(calls).toHaveLength(1);
  });

  it('ignores presence for a meeting or person it does not list', async () => {
    const { store } = await loaded();
    const before = JSON.stringify(store.meetings);
    store.applyPresence('0190a4e0-0000-7000-8000-0000000000ff', BOB.id, true);
    store.applyPresence(MEETING_ID, '0190a4e0-0000-7000-8000-0000000000ee', true);
    expect(JSON.stringify(store.meetings)).toBe(before);
  });

  it('does nothing before a list is loaded', () => {
    const store = useMeetingsStore();
    expect(() => store.applyPresence(MEETING_ID, BOB.id, true)).not.toThrow();
    expect(store.meetings).toEqual([]);
  });
});
