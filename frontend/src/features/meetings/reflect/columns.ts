import { ReflectionColumn } from '@/api/client';

/** The Reflect columns, in board order (spec §9.2). */
export const COLUMNS: ReflectionColumn[] = [
  ReflectionColumn.plus,
  ReflectionColumn.delta,
  ReflectionColumn.ideas,
  ReflectionColumn.thanks,
];

/** Most mini cards drawn in a column's team strip; the count says the rest. */
export const MAX_MINI_CARDS = 12;
