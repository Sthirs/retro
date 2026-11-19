import { createRouter, createWebHistory, type Router, type RouterHistory } from 'vue-router';

import { safeRedirect } from '@/lib/redirect';
import { useSessionStore } from '@/stores/session';
import { useTeamsStore } from '@/stores/teams';

declare module 'vue-router' {
  interface RouteMeta {
    /** Only for signed-in users; others go to /login. */
    requiresAuth?: boolean;
    /** Only for signed-out users; others go to their redirect target. */
    guestOnly?: boolean;
    /** The team menu entry (a route name) this page belongs to. */
    section?: string;
  }
}

export function buildRouter(history: RouterHistory = createWebHistory()): Router {
  const router = createRouter({
    history,
    routes: [
      {
        path: '/login',
        name: 'login',
        component: () => import('@/features/auth/SignInView.vue'),
        meta: { guestOnly: true },
      },
      {
        path: '/auth/callback',
        name: 'auth-callback',
        component: () => import('@/features/auth/AuthCallbackView.vue'),
      },
      {
        // The last used team, or onboarding when the user has none (spec §15.2).
        path: '/',
        name: 'home',
        component: () => import('@/features/onboarding/OnboardingView.vue'),
        meta: { requiresAuth: true },
        beforeEnter: async () => {
          const teams = useTeamsStore();
          try {
            await teams.ensureLoaded();
          } catch {
            return true;
          }
          const team = teams.defaultTeam();
          return team ? { name: 'team-meetings', params: { teamId: team.id } } : true;
        },
      },
      {
        path: '/invite/:token',
        name: 'invitation',
        component: () => import('@/features/invitations/InvitationView.vue'),
        meta: { requiresAuth: true },
      },
      {
        path: '/join/:joinToken',
        name: 'join',
        component: () => import('@/features/invitations/JoinView.vue'),
        meta: { requiresAuth: true },
      },
      {
        path: '/teams/:teamId',
        component: () => import('@/components/layout/AppShell.vue'),
        meta: { requiresAuth: true },
        children: [
          { path: '', redirect: (to) => ({ name: 'team-meetings', params: to.params }) },
          {
            path: 'meetings',
            name: 'team-meetings',
            component: () => import('@/features/meetings/MeetingsView.vue'),
            meta: { section: 'team-meetings' },
          },
          {
            // An ended meeting's summary sits in the shell (Summary artboard);
            // the room sends everyone here once the meeting ends.
            path: 'meetings/:meetingId/summary',
            name: 'meeting-summary',
            component: () => import('@/features/meetings/summary/MeetingSummaryView.vue'),
            meta: { section: 'team-meetings' },
          },
          {
            path: 'tasks',
            name: 'team-tasks',
            component: () => import('@/features/tasks/TasksBoardView.vue'),
            meta: { section: 'team-tasks' },
          },
          {
            path: 'settings',
            name: 'team-settings',
            component: () => import('@/features/teams/TeamSettingsView.vue'),
            meta: { section: 'team-settings' },
          },
        ],
      },
      {
        // The meeting room has its own chrome (phase sidebar, bottom bar), so
        // it sits outside the shell; an ended meeting redirects to its summary.
        path: '/teams/:teamId/meetings/:meetingId',
        name: 'meeting-room',
        component: () => import('@/features/meetings/MeetingRoomView.vue'),
        meta: { requiresAuth: true },
      },
      {
        // Opening a meeting invite link joins the team if needed, then the meeting (spec §6.3).
        path: '/m/:inviteToken',
        name: 'meeting-invite',
        component: () => import('@/features/invitations/MeetingInviteView.vue'),
        meta: { requiresAuth: true },
      },
      {
        // Not tied to a team: the shell falls back to the remembered team.
        path: '/account',
        component: () => import('@/components/layout/AppShell.vue'),
        meta: { requiresAuth: true },
        beforeEnter: async () => {
          try {
            await useTeamsStore().ensureLoaded();
          } catch {
            // The page works without teams; the shell just has no team menu.
          }
          return true;
        },
        children: [
          {
            path: '',
            name: 'account',
            component: () => import('@/features/account/AccountView.vue'),
          },
        ],
      },
      { path: '/:pathMatch(.*)*', redirect: '/' },
    ],
  });

  router.beforeEach(async (to) => {
    const session = useSessionStore();
    await session.restore();
    if (to.meta.requiresAuth && !session.isAuthenticated) {
      return { name: 'login', query: to.fullPath === '/' ? {} : { redirect: to.fullPath } };
    }
    if (to.meta.guestOnly && session.isAuthenticated) {
      return safeRedirect(to.query.redirect);
    }
    const teamId = to.params.teamId;
    if (typeof teamId === 'string') {
      // Only members see a team (spec §5.1): anything else goes home.
      const teams = useTeamsStore();
      try {
        await teams.ensureLoaded();
      } catch {
        return { name: 'home' };
      }
      if (!teams.byId(teamId)) return { name: 'home' };
      teams.remember(teamId);
    }
    return true;
  });

  return router;
}
