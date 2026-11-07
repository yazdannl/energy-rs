import { describe, expect, it } from 'vitest';

import { exportPrice, feedInPost, netMeteringActive, salderingHint } from './settlement';

describe('settlement preview', () => {
  it('follows the date-driven netting switch', () => {
    expect(netMeteringActive(new Date('2026-06-01T12:00:00Z'))).toBe(true);
    expect(netMeteringActive(new Date('2027-01-01T12:00:00Z'))).toBe(false);
    expect(salderingHint(new Date('2026-06-01T12:00:00Z'))).toContain('1:1 netting');
    expect(salderingHint(new Date('2027-06-01T12:00:00Z'))).toContain('ended');
  });

  it('computes the post-saldering feed-in from market plus fee', () => {
    expect(feedInPost(0.08)).toBeCloseTo(0.1, 6);
    expect(feedInPost(0.08, true)).toBeCloseTo(0.11, 6);
    expect(feedInPost(-0.3)).toBeCloseTo(-0.28, 6);
  });

  it('nets 1:1 while the regime applies and clamps at zero otherwise', () => {
    const slot = { buyEurPerKwh: 0.2412, marketEurPerKwh: 0.08 };
    expect(exportPrice(slot, true)).toBe(0.2412);
    expect(exportPrice(slot, false)).toBe(0.1);
    expect(exportPrice({ buyEurPerKwh: -0.01, marketEurPerKwh: -0.4 }, true)).toBe(0);
    expect(exportPrice({ buyEurPerKwh: 0.2, marketEurPerKwh: -0.4 }, false)).toBe(0);
  });
});
