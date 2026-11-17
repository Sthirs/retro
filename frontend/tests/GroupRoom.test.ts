import { flushPromises, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  ALICE,
  BOB,
  me,
  MEETING_ID,
  meetingState,
  mockFetch,
  mockSocket,
  openSocket,
  participant,
  problem,
  renderAt,
  session,
  team,
  TEAM_ID,
  wsTicket,
} from './helpers';

const BASE = `/api/v1/meetings/${MEETING_ID}`;
const channel = `meeting:${MEETING_ID}`;
const mounted: VueWrapper[] = [];
afterEach(() => {
  for (const w of mounted.splice(0)) w.unmount();
  vi.restoreAllMocks();
  localStorage.clear();
  sessionStorage.clear();
});
beforeEach(() => mockSocket());

const card = (id: string, column: string, position: number, text: string, overrides = {}) => ({
  id,
  column,
  content: `<p>${text}</p>`,
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
    participants: [participant(ALICE), participant(BOB)],
    reflections: [
      card('p1', 'plus', 0, 'Release train shipped on time'),
      card('d1', 'delta', 0, 'Flaky e2e tests blocked merges', { groupId: 'g1' }),
      card('d2', 'delta', 1, 'CI takes 25 minutes', { groupId: 'g1' }),
      card('i1', 'ideas', 0, 'Quarantine flaky tests'),
      card('t1', 'thanks', 0, 'Thanks Marco'),
    ],
    groups: [{ id: 'g1', column: 'delta', position: 0, name: 'Flaky CI' }],
    ...overrides,
  });

function routes(state = grouping(), extra: Record<string, unknown> = {}) {
  return {
    'POST /api/v1/auth/refresh': session(ALICE),
    'GET /api/v1/me': me([team({ role: 'admin' })], [], ALICE),
    'POST /api/v1/ws-ticket': wsTicket(),
    [`GET /api/v1/teams/${TEAM_ID}/meetings`]: { status: 200, body: [] },
    [`POST ${BASE}/join`]: { status: 200, body: state },
    [`GET ${BASE}/state`]: { status: 200, body: state },
    ...extra,
  } as Parameters<typeof mockFetch>[0];
}

async function open() {
  const result = await renderAt(`/teams/${TEAM_ID}/meetings/${MEETING_ID}`);
  mounted.push(result.wrapper);
  await flushPromises();
  return result;
}

const column = (wrapper: VueWrapper, name: string) => wrapper.get(`section[aria-label="${name}"]`);
const cardEl = (wrapper: VueWrapper, id: string) => wrapper.get(`[data-card-id="${id}"]`);

describe('Group phase', () => {
  it('shows every card in its column and groups collapsed with a preview', async () => {
    mockFetch(routes());
    const { wrapper } = await open();

    expect(wrapper.get('h1').text()).toBe('Group');
    expect(column(wrapper, 'Plus').text()).toContain('Release train shipped on time');
    const group = wrapper.get('section[aria-label="Flaky CI"]');
    expect(group.get('input').element.value).toBe('Flaky CI');
    expect(group.text()).toContain('2 cards');
    // Collapsed: a preview of the first card, not the cards.
    expect(group.find('[data-card-id]').exists()).toBe(false);
    expect(group.text()).toContain('Flaky e2e tests blocked merges');

    await group.get('button[aria-expanded="false"]').trigger('click');
    expect(group.findAll('[data-card-id]')).toHaveLength(2);
    await group.get('button[aria-label="Collapse group"]').trigger('click');
    expect(group.find('[data-card-id]').exists()).toBe(false);
  });

  it('renames a group on Enter, and Escape takes the edit back', async () => {
    const { calls } = mockFetch(
      routes(grouping(), {
        'PATCH /api/v1/groups/g1': { status: 200, body: grouping() },
      }),
    );
    const { wrapper } = await open();
    const input = wrapper.get('input[aria-label="Group name"]');

    await input.trigger('focus');
    await input.setValue('Something else');
    await input.trigger('keydown', { key: 'Escape' });
    expect((input.element as HTMLInputElement).value).toBe('Flaky CI');
    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);

    await input.trigger('focus');
    await input.setValue('  CI is slow ');
    await input.trigger('keydown', { key: 'Enter' });
    await input.trigger('blur');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PATCH')?.body).toEqual({ name: 'CI is slow' });
  });

  it('moves a card from the keyboard with Move to…', async () => {
    const moved = grouping({
      reflections: [
        card('p1', 'plus', 0, 'Release train shipped on time'),
        card('d1', 'delta', 0, 'Flaky e2e tests blocked merges', { groupId: 'g1' }),
        card('d2', 'delta', 1, 'CI takes 25 minutes', { groupId: 'g1' }),
        card('i1', 'ideas', 0, 'Quarantine flaky tests', { groupId: 'g1', version: 1 }),
        card('t1', 'thanks', 0, 'Thanks Marco'),
      ],
      seq: 3,
    });
    const { calls } = mockFetch(
      routes(grouping(), { 'PUT /api/v1/reflections/i1/group': { status: 200, body: moved } }),
    );
    const { wrapper } = await open();
    const trigger = cardEl(wrapper, 'i1').get('[data-move-trigger]');
    await trigger.trigger('click');
    const menu = wrapper.get('[role="menu"]');
    const items = menu.findAll('[role="menuitem"]');
    // The group first, then the other loose cards to start a group with.
    expect(items.map((i) => i.text())).toEqual([
      expect.stringContaining('Flaky CI'),
      'Release train shipped on time',
      'Thanks Marco',
    ]);
    expect(document.activeElement).toBe(items[0]!.element);
    await menu.trigger('keydown', { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[1]!.element);
    await menu.trigger('keydown', { key: 'ArrowUp' });

    await items[0]!.trigger('click');
    await flushPromises();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ groupId: 'g1' });
    // The group opens on the moved card, which keeps the focus.
    const group = wrapper.get('section[aria-label="Flaky CI"]');
    expect(group.findAll('[data-card-id]')).toHaveLength(3);
    expect(document.activeElement).toBe(cardEl(wrapper, 'i1').get('[data-move-trigger]').element);
  });

  it('focuses the card again where it is when a move is refused', async () => {
    mockFetch(
      routes(grouping(), { 'PUT /api/v1/reflections/i1/group': problem(409, 'wrong_phase') }),
    );
    const { wrapper } = await open();
    await cardEl(wrapper, 'i1').get('[data-move-trigger]').trigger('click');
    // Into the collapsed group: the card's menu unmounts with it.
    await wrapper.get('[role="menu"]').findAll('[role="menuitem"]')[0]!.trigger('click');
    await flushPromises();
    const trigger = cardEl(wrapper, 'i1').get('[data-move-trigger]');
    expect(trigger.element.closest('section[aria-label="Flaky CI"]')).toBeNull();
    expect(document.activeElement).toBe(trigger.element);
  });

  it('keeps the focus on the toggle when a group expands or collapses', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const group = wrapper.get('section[aria-label="Flaky CI"]');
    await group.get('button[aria-expanded="false"]').trigger('click');
    await flushPromises();
    expect(document.activeElement).toBe(group.get('button[aria-label="Collapse group"]').element);
    await group.get('button[aria-label="Collapse group"]').trigger('click');
    await flushPromises();
    expect(document.activeElement).toBe(group.get('button[aria-expanded="false"]').element);
  });

  it('leaves the name alone while an IME composes', async () => {
    const { calls } = mockFetch(routes());
    const { wrapper } = await open();
    const input = wrapper.get('input[aria-label="Group name"]');
    (input.element as HTMLInputElement).focus();
    await input.setValue('Flaky');
    await input.trigger('keydown', { key: 'Enter', isComposing: true });
    await flushPromises();
    expect(document.activeElement).toBe(input.element);
    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);
  });

  it('shows who else is dragging a card and locks it', async () => {
    mockFetch(routes());
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();
    socket.emit({
      type: 'card.dragging',
      channel,
      payload: { drags: [{ reflectionId: 'p1', userId: BOB.id }] },
    });
    await flushPromises();
    const held = cardEl(wrapper, 'p1');
    expect(held.get('[data-testid="drag-badge"]').text()).toContain('Bob is moving');
    expect(held.get('[data-move-trigger]').attributes('disabled')).toBeDefined();

    socket.emit({ type: 'card.dragging', channel, payload: { drags: [] } });
    await flushPromises();
    expect(cardEl(wrapper, 'p1').find('[data-testid="drag-badge"]').exists()).toBe(false);
  });

  it('drops a card onto another card with the pointer and tells the others', async () => {
    const answer = grouping({ seq: 3 });
    const { calls } = mockFetch(
      routes(grouping(), { [`POST ${BASE}/groups`]: { status: 200, body: answer } }),
    );
    const { wrapper } = await open();
    const socket = await openSocket();
    socket.emit({ type: 'subscribed', channel });
    await flushPromises();

    const source = cardEl(wrapper, 'i1').element as HTMLElement;
    const target = cardEl(wrapper, 'p1').element as HTMLElement;
    vi.spyOn(document, 'elementFromPoint').mockReturnValue(target);
    const pointer = (type: string, x: number, y: number) =>
      new PointerEvent(type, {
        bubbles: true,
        button: 0,
        isPrimary: true,
        pointerId: 1,
        pointerType: 'mouse',
        clientX: x,
        clientY: y,
      });
    source.dispatchEvent(pointer('pointerdown', 10, 10));
    document.dispatchEvent(pointer('pointermove', 30, 40));
    await flushPromises();
    expect(socket.sent.at(-1)).toEqual({ type: 'drag', channel, reflectionId: 'i1' });
    expect(target.classList.contains('gcard--over')).toBe(true);

    document.dispatchEvent(pointer('pointerup', 30, 40));
    await flushPromises();
    expect(calls.find((c) => c.method === 'POST' && c.path === `${BASE}/groups`)?.body).toEqual({
      targetId: 'p1',
      reflectionId: 'i1',
    });
    expect(socket.sent.at(-1)).toEqual({ type: 'drag', channel, reflectionId: null });
    expect(document.querySelector('[data-drag-ghost]')).toBeNull();
  });

  it('takes a dragged card out of its group when dropped on the column', async () => {
    const { calls } = mockFetch(
      routes(grouping(), {
        'PUT /api/v1/reflections/d2/group': { status: 200, body: grouping({ seq: 3 }) },
      }),
    );
    const { wrapper } = await open();
    await wrapper
      .get('section[aria-label="Flaky CI"] button[aria-expanded="false"]')
      .trigger('click');

    const source = cardEl(wrapper, 'd2').element as HTMLElement;
    vi.spyOn(document, 'elementFromPoint').mockReturnValue(column(wrapper, 'Thanks').element);
    const init = { bubbles: true, button: 0, isPrimary: true, pointerId: 2, pointerType: 'mouse' };
    source.dispatchEvent(new PointerEvent('pointerdown', { ...init, clientX: 0, clientY: 0 }));
    document.dispatchEvent(new PointerEvent('pointermove', { ...init, clientX: 50, clientY: 0 }));
    document.dispatchEvent(new PointerEvent('pointerup', { ...init, clientX: 50, clientY: 0 }));
    await flushPromises();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ groupId: null });
  });
});
