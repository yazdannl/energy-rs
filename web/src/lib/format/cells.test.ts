import { describe, expect, it } from 'vitest';

import type { PackCells } from '../api/types';

import { cellBadge, formatCells } from './cells';

const reported: PackCells = {
  minCellV: 3.32,
  maxCellV: 3.35,
  spreadMv: 30,
  packVoltageV: 49.7,
  maxTempC: 29.1,
  level: 'ok',
};

describe('formatCells', () => {
  it('renders the highest cell, lowest cell, spread, pack voltage and temperature', () => {
    expect(formatCells(reported)).toBe('3.35 / 3.32 V · Δ 30 mV · pack 49.7 V · 29.1 °C');
  });

  it('shows one em dash when the pack published no packData', () => {
    const empty: PackCells = {
      minCellV: null,
      maxCellV: null,
      spreadMv: null,
      packVoltageV: null,
      maxTempC: null,
      level: null,
    };
    expect(formatCells(empty)).toBe('—');
    expect(formatCells(null)).toBe('—');
    expect(formatCells(undefined)).toBe('—');
  });

  it('keeps the readings it has and never prints NaN or null for the missing ones', () => {
    const text = formatCells({ ...reported, minCellV: null, spreadMv: null });
    expect(text).toBe('3.35 / — V · Δ — mV · pack 49.7 V · 29.1 °C');
    expect(text).not.toContain('NaN');
    expect(text).not.toContain('null');
  });
});

describe('cellBadge', () => {
  it('uses the guard bands for the badge tone', () => {
    expect(cellBadge('below_cutoff')).toEqual({ tone: 'bad', label: 'below cutoff' });
    expect(cellBadge('below_resume')).toEqual({ tone: 'warn', label: 'below resume' });
    expect(cellBadge('ok')).toEqual({ tone: 'good', label: 'cells ok' });
    expect(cellBadge(null)).toBeNull();
  });
});
