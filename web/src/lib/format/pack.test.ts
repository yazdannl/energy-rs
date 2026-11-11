import { describe, expect, it } from 'vitest';

import { packLabel } from './pack';

describe('packLabel', () => {
  it('prefers the operator alias over the discovered name and the serial', () => {
    expect(packLabel({ sn: 'AB2000-A1', name: 'Attic A', alias: 'Left shelf' })).toBe('Left shelf');
  });

  it('falls back to the discovered name, then the serial', () => {
    expect(packLabel({ sn: 'AB2000-A1', name: 'Attic A', alias: null })).toBe('Attic A');
    expect(packLabel({ sn: 'AB2000-A1', name: null, alias: null })).toBe('AB2000-A1');
  });
});
