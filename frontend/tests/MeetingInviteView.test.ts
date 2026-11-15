import { describe, expect, it, vi } from 'vitest';

import {
  me,
  MEETING_ID,
  meetingState,
  mockFetch,
  mockSocket,
  problem,
  renderAt,
  session,
  team,
  TEAM_ID,
  wsTicket,
} from './helpers';

const accepted = (joinedTeam: boolean) => ({
  status: 200,
  body: { teamId: TEAM_ID, meetingId: MEETING_ID, title: 'Retro #3', joinedTeam },
});

describe('Meeting invite link', () => {
  it('adds a newcomer to the team, then opens the room', async () => {
    await mockSocket();
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/meeting-invites/tok/accept': accepted(true),
      'GET /api/v1/me': me([team({ role: 'member' })]),
      'POST /api/v1/ws-ticket': wsTicket(),
      [`POST /api/v1/meetings/${MEETING_ID}/join`]: { status: 200, body: meetingState() },
    });
    const { wrapper, router } = await renderAt('/m/tok');
    expect(wrapper.find('h1').text()).toBe('Joining Retro #3…');
    expect(wrapper.text()).toContain("You've been added to Platform Team as a member.");
    expect(wrapper.find('[role="progressbar"]').exists()).toBe(true);
    await vi.waitFor(
      () =>
        expect(router.currentRoute.value.fullPath).toBe(`/teams/${TEAM_ID}/meetings/${MEETING_ID}`),
      { timeout: 3000 },
    );
    wrapper.unmount();
  });

  it('takes a member straight into the room', async () => {
    await mockSocket();
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/meeting-invites/tok/accept': accepted(false),
      'GET /api/v1/me': me([team()]),
      'POST /api/v1/ws-ticket': wsTicket(),
      [`POST /api/v1/meetings/${MEETING_ID}/join`]: { status: 200, body: meetingState() },
    });
    const { wrapper, router } = await renderAt('/m/tok');
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe('meeting-room'));
    wrapper.unmount();
  });

  it('explains an ended or replaced link', async () => {
    mockFetch({
      'POST /api/v1/auth/refresh': session(),
      'POST /api/v1/meeting-invites/old/accept': problem(404, 'meeting_invite_invalid'),
    });
    const { wrapper } = await renderAt('/m/old');
    expect(wrapper.find('h1').text()).toBe('This meeting has ended');
    expect(wrapper.text()).toContain('Meeting invite links stop working when the meeting ends.');
    wrapper.unmount();
  });
});
