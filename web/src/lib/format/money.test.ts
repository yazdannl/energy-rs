import { describe, expect, it } from 'vitest';

import { fmtEur, fmtEur6, round6 } from './money';

/** nl-NL uses a comma and a narrow no-break space; normalise so the assertion is about digits. */
const clean = (value: string) => value.replace(/\u00a0|\u202f/g, ' ');

describe('money formatting', () => {
  it('shows two decimals and no more', () => {
    expect(clean(fmtEur(1.234))).toBe('€ 1,23');
    expect(clean(fmtEur(0.005))).toBe('€ 0,01');
  });

  it('shows the ledger resolution when asked for it', () => {
    expect(clean(fmtEur6(0.1234565))).toBe('€ 0,123457');
    expect(clean(fmtEur6(0.2))).toBe('€ 0,200000');
  });

  it('renders a missing amount as an em dash, never as zero', () => {
    expect(fmtEur(null)).toBe('—');
    expect(fmtEur6(undefined)).toBe('—');
  });

  it('rounds to the API resolution only when asked', () => {
    expect(round6(0.30000000000000004)).toBe(0.3);
    expect(round6(0.1234565)).toBe(0.123457);
  });
});
