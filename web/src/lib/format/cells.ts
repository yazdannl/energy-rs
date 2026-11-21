/**
 * Cell-voltage display for one pack.
 *
 * The device publishes only the highest and lowest cell of a pack, never every cell. A pack can
 * also publish no `packData` at all while full, so every part degrades to the shared em dash.
 */

import type { CellLevel, PackCells } from '../api/types';
import type { Tone } from '../uiTone';

import { NO_VALUE, orNo } from './empty';

function finite(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

/** `3.35 / 3.32 V · Δ 30 mV · pack 49.7 V · 29.1 °C`, or `—` with no packData. */
export function formatCells(cells: PackCells | null | undefined): string {
  if (!cells) return NO_VALUE;
  const min = finite(cells.minCellV);
  const max = finite(cells.maxCellV);
  const spread = finite(cells.spreadMv);
  const voltage = finite(cells.packVoltageV);
  const temp = finite(cells.maxTempC);
  if ([min, max, spread, voltage, temp].every((value) => value === null)) return NO_VALUE;
  return `${orNo(max, (v) => v.toFixed(2))} / ${orNo(min, (v) => v.toFixed(2))} V · Δ ${orNo(spread, (v) => Math.round(v).toString())} mV · pack ${orNo(voltage, (v) => v.toFixed(1))} V · ${orNo(temp, (v) => v.toFixed(1))} °C`;
}

/** The badge the guard's own `level` earns; no level means no badge. */
export function cellBadge(
  level: CellLevel | null | undefined,
): { tone: Tone; label: string } | null {
  switch (level) {
    case 'below_cutoff':
      return { tone: 'bad', label: 'below cutoff' };
    case 'below_resume':
      return { tone: 'warn', label: 'below resume' };
    case 'ok':
      return { tone: 'good', label: 'cells ok' };
    default:
      return null;
  }
}
