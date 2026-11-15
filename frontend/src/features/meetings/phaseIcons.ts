import type { MeetingPhase } from '@/api/client';
import type { IconName } from '@/components/icons';

/** The icon of each phase in the stepper (meeting room artboards). */
export const PHASE_ICONS: Record<MeetingPhase, IconName> = {
  icebreaker: 'users',
  reflect: 'pencil',
  group: 'grid',
  vote: 'thumbsUp',
  discuss: 'message',
};
