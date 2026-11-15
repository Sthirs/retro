import type { Page } from '@playwright/test';

/**
 * Resolves when the server acknowledges the page's subscription to `channel`
 * (its `subscribed` frame). Call before the navigation that opens the socket.
 */
export function channelSubscribed(page: Page, channel: string): Promise<void> {
  return new Promise((resolve) => {
    page.on('websocket', (socket) => {
      socket.on('framereceived', ({ payload }) => {
        if (typeof payload !== 'string') return;
        try {
          const message = JSON.parse(payload) as { type?: string; channel?: string };
          if (message.type === 'subscribed' && message.channel === channel) resolve();
        } catch {
          // Not JSON: not ours.
        }
      });
    });
  });
}
