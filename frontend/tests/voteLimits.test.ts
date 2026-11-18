import { describe, expect, it } from 'vitest';

import { BUDGET, PER_GROUP } from '@/features/meetings/vote/voteLimits';

import openapi from '../openapi.json';

describe('vote limits', () => {
  it('match the bounds and defaults the server documents', () => {
    const fields = openapi.components.schemas.VoteSettingsRequest.properties;
    for (const [limits, field] of [
      [BUDGET, fields.voteBudget],
      [PER_GROUP, fields.voteMaxPerGroup],
    ] as const) {
      expect({ min: limits.min, max: limits.max, default: limits.default }).toEqual({
        min: field.minimum,
        max: field.maximum,
        default: field.default,
      });
    }
  });
});
