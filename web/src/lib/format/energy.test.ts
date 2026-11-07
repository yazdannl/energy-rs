import { describe, expect, it } from 'vitest';

import { fmtConfidence, fmtKwh, fmtPct, fmtW, fmtWh } from './energy';

const number = (value: string) => Number(value.replace(/[^\d,.-]/g, '').replace(',', '.'));

describe('energy formatting', () => {
  it('switches from Wh to kWh at one kilowatt-hour', () => {
    expect(fmtWh(250)).toBe('250 Wh');
    expect(number(fmtWh(2500))).toBe(2.5);
    expect(fmtWh(2500).endsWith(' kWh')).toBe(true);
  });

  it('keeps kWh fields in kWh', () => {
    expect(fmtKwh(1.85).endsWith(' kWh')).toBe(true);
    expect(number(fmtKwh(1.85))).toBe(1.85);
  });

  it('prints watts below one kilowatt and kW above', () => {
    expect(fmtW(412)).toBe('412 W');
    expect(fmtW(-120)).toBe('-120 W');
    expect(number(fmtW(1240))).toBe(1.24);
    expect(fmtW(1240).endsWith(' kW')).toBe(true);
  });

  it('handles a missing measurement without inventing a zero', () => {
    expect(fmtW(null)).toBe('—');
    expect(fmtWh(undefined)).toBe('—');
    expect(fmtPct(null)).toBe('—');
  });

  it('turns a 0..1 confidence into a percentage', () => {
    expect(fmtConfidence(0.82)).toBe('82 %');
    expect(fmtConfidence(null)).toBe('—');
  });
});
