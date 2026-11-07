import { describe, expect, it } from 'vitest';

import { makeSavings } from './fixtures';

describe('savings fixtures', () => {
  it('splits a measured day into four components that sum to the saving', () => {
    const savings = makeSavings(new Date('2026-06-01T12:00:00Z'), 'month');
    const complete = savings.days.filter((day) => day.complete && day.savingEur !== null);

    expect(complete.length).toBeGreaterThan(0);
    for (const day of complete) {
      const sum =
        (day.arbitrageEur ?? 0) +
        (day.selfConsumptionEur ?? 0) +
        (day.feedInEur ?? 0) +
        (day.batteryBalanceEur ?? 0);
      expect(sum).toBeCloseTo(day.savingEur ?? 0, 6);
    }
  });

  it('reports no money for a closed day with a gap, but keeps its coverage', () => {
    const savings = makeSavings(new Date('2026-06-01T12:00:00Z'), 'month');
    const gapped = savings.days.filter((day) => !day.complete && day.savingEur === null);

    expect(gapped.length).toBeGreaterThan(0);
    for (const day of gapped) {
      expect(day.actualEur).toBeNull();
      expect(day.arbitrageEur).toBeNull();
      expect(day.coverageHours).toBeGreaterThan(0);
      expect(day.importWh).toBeGreaterThan(0);
    }
  });

  it('keeps the current day provisional with money present', () => {
    const savings = makeSavings(new Date('2026-06-01T12:00:00Z'), 'day');
    const today = savings.days.at(-1);

    expect(today?.complete).toBe(false);
    expect(today?.savingEur).not.toBeNull();
    expect(savings.totals.savingEur).toBeGreaterThan(0);
    expect(savings.totals.days).toBe(1);
  });

  it('excludes null days from totals but counts them as rows', () => {
    const savings = makeSavings(new Date('2026-06-01T12:00:00Z'), 'month');
    const moneyDays = savings.days.filter((day) => day.savingEur !== null).length;

    expect(savings.totals.days).toBeGreaterThan(moneyDays);
    expect(savings.totals.completeDays).toBeLessThan(savings.totals.days);
  });
});
