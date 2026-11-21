import { describe, expect, it } from 'vitest';

import type { PackCells } from '../api/types';

import {
  CELL_STALE_AFTER_S,
  cellAge,
  formatCellPair,
  formatCells,
  formatPackContext,
} from './cells';

const reported: PackCells = {
  minCellV: 3.32,
  maxCellV: 3.35,
  spreadMv: 30,
  packVoltageV: 49.7,
  maxTempC: 29.1,
};

const sampledAt = '2026-06-01T12:00:00Z';
const now = Date.parse(sampledAt);

describe('cell formatting', () => {
  it('renders the highest cell, lowest cell, spread, pack voltage and temperature', () => {
    expect(formatCells(reported)).toBe('3.35 / 3.32 V · Δ 30 mV · pack 49.7 V · 29.1 °C');
  });

  it('splits into two short rows so narrow cards keep the numbers visible', () => {
    expect(formatCellPair(reported)).toBe('3.35 / 3.32 V · Δ 30 mV');
    expect(formatPackContext(reported)).toBe('pack 49.7 V · 29.1 °C');
  });

  it('shows one em dash when the pack published no packData', () => {
    const empty: PackCells = {
      minCellV: null,
      maxCellV: null,
      spreadMv: null,
      packVoltageV: null,
      maxTempC: null,
    };
    expect(formatCells(empty)).toBe('—');
    expect(formatCells(null)).toBe('—');
    expect(formatCells(undefined)).toBe('—');
  });

  it('keeps the readings it has and never prints NaN or null for the missing ones', () => {
    const partial = { ...reported, minCellV: null, spreadMv: null };
    const text = formatCells(partial);
    expect(formatCellPair(partial)).toBe('3.35 / — V · Δ — mV');
    expect(formatPackContext(partial)).toBe('pack 49.7 V · 29.1 °C');
    expect(text).toBe('3.35 / — V · Δ — mV · pack 49.7 V · 29.1 °C');
    expect(text).not.toContain('NaN');
    expect(text).not.toContain('null');
  });
});

describe('cellAge', () => {
  it('uses the same sampledAt age as the state and stays fresh inside three ticks', () => {
    expect(CELL_STALE_AFTER_S).toBe(15);
    expect(cellAge(sampledAt, now + 4_000)).toEqual({ label: '4 s ago', stale: false });
    expect(cellAge(sampledAt, now + 15_000).stale).toBe(false);
  });

  it('marks a reading older than three engine ticks as stale', () => {
    expect(cellAge(sampledAt, now + 22_000)).toEqual({ label: '22 s ago', stale: true });
    expect(cellAge(sampledAt, now + 150_000)).toEqual({ label: '2 min ago', stale: true });
  });

  it('has no invented age when the pack never sampled', () => {
    expect(cellAge(null, now)).toEqual({ label: '—', stale: true });
  });
});
