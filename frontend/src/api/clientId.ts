/**
 * This tab's id, sent as `X-Client-Id` with every API request. The server
 * echoes it as `clientId` in the real-time events of the changes the tab made,
 * so the tab can skip its own echo (it already applied the change from the
 * answer) while the user's other tabs still update (spec §11.2, ADR-0011).
 */
export const CLIENT_ID: string = newId();

function newId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  // `randomUUID` needs a secure context; build a version 4 UUID by hand.
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** Whether a real-time event was caused by this tab. */
export function isOwnEvent(payload: { clientId?: unknown } | null | undefined): boolean {
  return payload?.clientId === CLIENT_ID;
}
