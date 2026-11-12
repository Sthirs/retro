/** Number of avatar background tokens (`--avatar-1` … `--avatar-6`). */
export const AVATAR_COLORS = 6;

/** Stable colour index (1-based) for a user id, so a user keeps their colour. */
export function avatarColorIndex(userId: string): number {
  let hash = 0;
  for (let i = 0; i < userId.length; i++) {
    hash = (hash * 31 + userId.charCodeAt(i)) >>> 0;
  }
  return (hash % AVATAR_COLORS) + 1;
}

/** Up to two initials from a display name: "Alice Rossi" → "AR". */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  const first = parts[0]?.[0] ?? '?';
  const last = parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? '') : '';
  return (first + last).toUpperCase();
}
