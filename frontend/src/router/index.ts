import { createRouter, createWebHistory, type Router, type RouterHistory } from 'vue-router';

import { safeRedirect } from '@/lib/redirect';
import { useSessionStore } from '@/stores/session';

declare module 'vue-router' {
  interface RouteMeta {
    /** Only for signed-in users; others go to /login. */
    requiresAuth?: boolean;
    /** Only for signed-out users; others go to their redirect target. */
    guestOnly?: boolean;
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
        path: '/',
        name: 'home',
        component: () => import('@/features/home/HomeView.vue'),
        meta: { requiresAuth: true },
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
    return true;
  });

  return router;
}
