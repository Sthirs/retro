import { describe, expect, it } from 'vitest';

import { formatClock, MAX_TIMER_SECONDS, parseDuration } from '@/lib/clock';

describe('clock', () => {
  it('formats a countdown as mm:ss, rounding up', () => {
    expect(formatClock(0)).toBe('00:00');
    expect(formatClock(-5)).toBe('00:00');
    expect(formatClock(1)).toBe('00:01');
    expect(formatClock(272_000)).toBe('04:32');
    expect(formatClock(271_001)).toBe('04:32');
    expect(formatClock(MAX_TIMER_SECONDS * 1000)).toBe('99:59');
  });

  it('parses typed durations', () => {
    expect(parseDuration('05:00')).toBe(300);
    expect(parseDuration(' 5:30 ')).toBe(330);
    expect(parseDuration('00:05')).toBe(5);
    expect(parseDuration('10')).toBe(600);
    expect(parseDuration('99:59')).toBe(MAX_TIMER_SECONDS);
  });

  it('rejects what is not a duration between 1 s and 99:59', () => {
    for (const text of ['', '00:00', '0', '5:60', '100:00', 'abc', '5:5', '-1', '1:00:00']) {
      expect(parseDuration(text), text).toBeNull();
    }
  });
});
