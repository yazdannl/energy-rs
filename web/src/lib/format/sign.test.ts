import { describe, expect, it } from 'vitest';

import { fmtW } from './energy';
import { batteryFlow, flowVerb, gridFlow, moneyTone, socTone } from './sign';

describe('sign conventions', () => {
  it('reads positive grid power as import', () => {
    expect(gridFlow(1200)).toBe('import');
    expect(gridFlow(-1200)).toBe('export');
    expect(gridFlow(0)).toBe('idle');
    expect(gridFlow(null)).toBe('idle');
  });

  it('reads positive battery power as charging', () => {
    expect(batteryFlow(800)).toBe('charge');
    expect(batteryFlow(-800)).toBe('discharge');
    expect(batteryFlow(4)).toBe('idle');
  });

  it('keeps the sign in words and in numbers', () => {
    expect(flowVerb(batteryFlow(800))).toBe('charging');
    expect(flowVerb(gridFlow(-500))).toBe('exporting');
    expect(fmtW(412)).toBe('412 W');
  });

  it('tones a low state of charge as a warning', () => {
    expect(socTone(62)).toBe('good');
    expect(socTone(22)).toBe('warn');
    expect(socTone(9)).toBe('bad');
    expect(socTone(null)).toBe('muted');
  });

  it('tones money by sign', () => {
    expect(moneyTone(0.5)).toBe('good');
    expect(moneyTone(-0.5)).toBe('bad');
    expect(moneyTone(0)).toBe('muted');
  });
});
