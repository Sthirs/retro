/**
 * Dragging cards in the Group phase (spec §9.3) with pointer events: a card
 * can be dropped *onto* another card or a group, which list-sorting libraries
 * (SortableJS) do not model.
 *
 * A mouse or pen drag starts after a few pixels; a touch drag after a short
 * hold, so a swipe still scrolls the page. While a card is lifted, a copy
 * follows the pointer and the drop target under it is tracked through
 * `data-drop` attributes: `card:{id}`, `group:{id}` or `column:{name}`.
 * Escape cancels.
 */
import { inject, onBeforeUnmount, provide, ref, type InjectionKey, type Ref } from 'vue';

export type DropTarget =
  { kind: 'card'; id: string } | { kind: 'group'; id: string } | { kind: 'column'; id: string };

export interface CardDrag {
  /** The card this tab is dragging. */
  dragging: Ref<string | null>;
  /** The drop target under the pointer. */
  over: Ref<DropTarget | null>;
  /** Starts tracking a press on a card. */
  press: (event: PointerEvent, cardId: string, element: HTMLElement) => void;
}

/** Pixels a mouse or pen moves before a drag starts. */
const MOUSE_THRESHOLD = 4;
/** How long a touch holds still before the card lifts. */
const TOUCH_HOLD_MS = 200;
/** Pixels a touch may wander during the hold (else it is a scroll). */
const TOUCH_SLOP = 8;

const KEY: InjectionKey<CardDrag> = Symbol('cardDrag');

/** Reads a `data-drop` attribute. */
export function parseDrop(value: string | undefined): DropTarget | null {
  const [kind, ...rest] = (value ?? '').split(':');
  const id = rest.join(':');
  if (!id) return null;
  if (kind === 'card' || kind === 'group' || kind === 'column') return { kind, id };
  return null;
}

/** Controls that keep their own pointer behaviour: pressing them never drags. */
const INTERACTIVE = 'button, a, input, textarea, select, [contenteditable="true"]';

export function provideCardDrag(handlers: {
  /** A card was lifted (`id`) or put down (`null`). */
  onLift: (cardId: string | null) => void;
  /** A lifted card was dropped on a target (never on itself). */
  onDrop: (cardId: string, target: DropTarget | null) => void;
}): CardDrag {
  const dragging = ref<string | null>(null);
  const over = ref<DropTarget | null>(null);
  let pending: {
    cardId: string;
    element: HTMLElement;
    pointerId: number;
    touch: boolean;
    x: number;
    y: number;
    timer: ReturnType<typeof setTimeout> | null;
  } | null = null;
  let ghost: HTMLElement | null = null;
  let origin = { left: 0, top: 0 };

  function press(event: PointerEvent, cardId: string, element: HTMLElement): void {
    if (pending || dragging.value) return;
    if (event.button !== 0 || !event.isPrimary) return;
    if ((event.target as Element | null)?.closest(INTERACTIVE)) return;
    const touch = event.pointerType === 'touch';
    pending = {
      cardId,
      element,
      pointerId: event.pointerId,
      touch,
      x: event.clientX,
      y: event.clientY,
      timer: touch ? setTimeout(() => lift(), TOUCH_HOLD_MS) : null,
    };
    if (!touch) event.preventDefault();
    document.addEventListener('pointermove', onMove);
    document.addEventListener('pointerup', onUp);
    document.addEventListener('pointercancel', onCancel);
    document.addEventListener('keydown', onKey, true);
    // Once lifted, a touch drag must not scroll the page.
    document.addEventListener('touchmove', onTouchMove, { passive: false });
  }

  function lift(): void {
    if (!pending || dragging.value) return;
    if (pending.timer) clearTimeout(pending.timer);
    pending.timer = null;
    const rect = pending.element.getBoundingClientRect();
    origin = { left: rect.left, top: rect.top };
    const copy = pending.element.cloneNode(true) as HTMLElement;
    copy.removeAttribute('data-drop');
    copy.setAttribute('aria-hidden', 'true');
    Object.assign(copy.style, {
      position: 'fixed',
      left: `${rect.left}px`,
      top: `${rect.top}px`,
      width: `${rect.width}px`,
      margin: '0',
      zIndex: '1000',
      pointerEvents: 'none',
      transform: 'rotate(-2deg)',
      boxShadow: 'var(--shadow-menu)',
      borderColor: 'var(--accent)',
      opacity: '0.95',
    });
    copy.dataset.dragGhost = '';
    document.body.append(copy);
    ghost = copy;
    dragging.value = pending.cardId;
    handlers.onLift(pending.cardId);
  }

  function onMove(event: PointerEvent): void {
    if (!pending || event.pointerId !== pending.pointerId) return;
    const dx = event.clientX - pending.x;
    const dy = event.clientY - pending.y;
    if (!dragging.value) {
      const distance = Math.hypot(dx, dy);
      if (pending.touch) {
        // Moved before the hold ended: a scroll, not a drag.
        if (distance > TOUCH_SLOP) finish();
        return;
      }
      if (distance < MOUSE_THRESHOLD) return;
      lift();
    }
    if (ghost) {
      ghost.style.left = `${origin.left + dx}px`;
      ghost.style.top = `${origin.top + dy}px`;
    }
    over.value = targetAt(event.clientX, event.clientY);
  }

  function targetAt(x: number, y: number): DropTarget | null {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-drop]');
    const target = parseDrop(element?.dataset.drop);
    if (target?.kind === 'card' && target.id === dragging.value) return null;
    return target;
  }

  function onUp(event: PointerEvent): void {
    if (!pending || event.pointerId !== pending.pointerId) return;
    const cardId = dragging.value;
    const target = cardId ? targetAt(event.clientX, event.clientY) : null;
    finish();
    if (cardId) handlers.onDrop(cardId, target);
  }

  function onCancel(event: PointerEvent): void {
    if (pending && event.pointerId === pending.pointerId) finish();
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return;
    event.stopPropagation();
    finish();
  }

  function onTouchMove(event: TouchEvent): void {
    if (dragging.value) event.preventDefault();
  }

  function finish(): void {
    if (pending?.timer) clearTimeout(pending.timer);
    pending = null;
    ghost?.remove();
    ghost = null;
    over.value = null;
    document.removeEventListener('pointermove', onMove);
    document.removeEventListener('pointerup', onUp);
    document.removeEventListener('pointercancel', onCancel);
    document.removeEventListener('keydown', onKey, true);
    document.removeEventListener('touchmove', onTouchMove);
    if (dragging.value) {
      dragging.value = null;
      handlers.onLift(null);
    }
  }

  onBeforeUnmount(finish);

  const drag: CardDrag = { dragging, over, press };
  provide(KEY, drag);
  return drag;
}

/** The Group phase's drag controller. */
export function useCardDrag(): CardDrag {
  const drag = inject(KEY);
  if (!drag) throw new Error('useCardDrag needs provideCardDrag in an ancestor');
  return drag;
}
