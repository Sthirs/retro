/**
 * Bounds and defaults of the vote settings (spec §9.4). The server owns
 * them; a test checks these against the OpenAPI document it generates.
 */
export interface VoteLimit {
  readonly default: number;
  readonly min: number;
  readonly max: number;
}

export const BUDGET: VoteLimit = { default: 5, min: 1, max: 20 };
export const PER_GROUP: VoteLimit = { default: 3, min: 1, max: 10 };
