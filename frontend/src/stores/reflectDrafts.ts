import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import { MeetingPhase, type ReflectionColumn } from '@/api/client';
import { realtime } from '@/api/realtime';
import { useMeetingStore } from '@/stores/meeting';
import { useSessionStore } from '@/stores/session';

/** The editor open in the Reflect phase: one at a time (Reflect artboard). */
export type OpenEditor =
  | { mode: 'new'; column: ReflectionColumn }
  | { mode: 'edit'; column: ReflectionColumn; reflectionId: string };

const KEY = 'retro.reflectDrafts';

/** Whether markup holds anything a reader would see: text or an uploaded image. */
export function hasContent(html: string): boolean {
  // A parsed document is inert: its images are never fetched.
  const body = new DOMParser().parseFromString(html, 'text/html').body;
  const text = body.textContent ?? '';
  return (
    text.replace(/\u00a0/g, ' ').trim() !== '' ||
    body.querySelector('img[data-attachment-id]') !== null
  );
}

/**
 * Reflect drafts (spec §9.2): they never leave the author's browser. Each
 * column's unsent text is kept in localStorage, per meeting and user, so a
 * reload keeps it; it reaches the server only when sent.
 *
 * The open editor is announced on the meeting channel as the tab's writing
 * state (ADR-0012): that is what "being written" means for everyone else.
 * When the meeting leaves Reflect or ends, the drafts are discarded.
 */
export const useReflectDraftsStore = defineStore('reflectDrafts', () => {
  const meetingStore = useMeetingStore();
  const session = useSessionStore();
  const drafts = ref<Partial<Record<ReflectionColumn, string>>>({});
  const open = ref<OpenEditor | null>(null);
  /** The meeting and user the drafts belong to. */
  let owner: string | null = null;

  const storageKey = (meetingId: string, userId: string) => `${KEY}.${meetingId}.${userId}`;

  function load(meetingId: string, userId: string): void {
    owner = storageKey(meetingId, userId);
    try {
      const raw = localStorage.getItem(owner);
      drafts.value = raw ? (JSON.parse(raw) as Partial<Record<ReflectionColumn, string>>) : {};
    } catch {
      drafts.value = {};
    }
  }

  /**
   * Saves one column (`null`: every column), against what is stored now:
   * another tab of the same meeting may have saved a different column.
   */
  function persist(column: ReflectionColumn | null): void {
    if (!owner) return;
    try {
      let stored: Partial<Record<ReflectionColumn, string>> = {};
      if (column) {
        const raw = localStorage.getItem(owner);
        stored = raw ? (JSON.parse(raw) as Partial<Record<ReflectionColumn, string>>) : {};
        const html = drafts.value[column];
        if (html && hasContent(html)) stored[column] = html;
        else delete stored[column];
      }
      if (Object.keys(stored).length === 0) localStorage.removeItem(owner);
      else localStorage.setItem(owner, JSON.stringify(stored));
    } catch {
      // Storage unavailable (private mode): drafts live as long as the tab.
    }
  }

  /** Columns holding unsent text. */
  const unsent = computed(
    () => Object.values(drafts.value).filter((html) => html && hasContent(html)).length,
  );

  function draft(column: ReflectionColumn): string {
    return drafts.value[column] ?? '';
  }

  function setDraft(column: ReflectionColumn, html: string): void {
    drafts.value = { ...drafts.value, [column]: html };
    persist(column);
  }

  function clearDraft(column: ReflectionColumn): void {
    const next = { ...drafts.value };
    delete next[column];
    drafts.value = next;
    persist(column);
  }

  function startNew(column: ReflectionColumn): void {
    open.value = { mode: 'new', column };
  }

  function startEdit(reflectionId: string, column: ReflectionColumn): void {
    open.value = { mode: 'edit', column, reflectionId };
  }

  function close(): void {
    open.value = null;
  }

  const channel = computed(() =>
    meetingStore.meetingId ? `meeting:${meetingStore.meetingId}` : null,
  );

  // Tell the meeting what this tab is writing. Leaving the channel (the room
  // closes) or Reflect ends it.
  watch(
    [open, channel, () => meetingStore.meeting?.phase, () => meetingStore.isActive],
    ([editor, name, phase], [, oldName]) => {
      if (oldName && oldName !== name) realtime.setWriting(oldName, null);
      if (!name) return;
      const writing = editor && phase === MeetingPhase.reflect && meetingStore.isActive;
      realtime.setWriting(name, writing ? { column: editor.column, mode: editor.mode } : null);
    },
  );

  // Drafts belong to one meeting and user.
  watch(
    () => [meetingStore.meetingId, session.user?.id] as const,
    ([meetingId, userId]) => {
      open.value = null;
      if (meetingId && userId) load(meetingId, userId);
      else {
        owner = null;
        drafts.value = {};
      }
    },
    { immediate: true },
  );

  // Moving past Reflect discards the drafts, and says so.
  watch(
    () => meetingStore.meeting?.phase,
    (phase, before) => {
      if (before !== MeetingPhase.reflect || phase === MeetingPhase.reflect || !phase) return;
      const count = discard();
      if (count > 0) meetingStore.notice = { kind: 'draftsDiscarded', count };
    },
  );

  // So does the meeting ending (the room then shows the ended meeting), or
  // opening one that has ended.
  watch(
    () => !!meetingStore.meeting && !meetingStore.isActive,
    (ended) => {
      if (ended) discard();
    },
    { immediate: true },
  );

  /** Closes the editor and drops every draft; returns how many had text. */
  function discard(): number {
    open.value = null;
    const count = unsent.value;
    drafts.value = {};
    persist(null);
    return count;
  }

  return {
    drafts,
    open,
    unsent,
    draft,
    setDraft,
    clearDraft,
    startNew,
    startEdit,
    close,
  };
});
