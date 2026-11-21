/**
 * Cell-voltage display for one pack.
 *
 * The device publishes only the highest and lowest cell of a pack, never every cell. A pack can
 * also publish no `packData` at all while full, so every part degrades to the shared em dash.
 * These are raw readings; whether the pack is protected is the latch's separate statement.
 */

import type { PackCells } from '../api/types';

import { NO_VALUE, orNo } from './empty';
import { ago } from './time';

/** Three engine ticks at the shipped 5 s tick: past this, a pack has stopped reporting. */
export const CELL_STALE_AFTER_S = 15;

function finite(value: number | null | undefined): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function readings(cells: PackCells | null | undefined) {
  return {
    min: finite(cells?.minCellV),
    max: finite(cells?.maxCellV),
    spread: finite(cells?.spreadMv),
    voltage: finite(cells?.packVoltageV),
    temp: finite(cells?.maxTempC),
  };
}

function missing(cells: PackCells | null | undefined): boolean {
  const values = readings(cells);
  return Object.values(values).every((value) => value === null);
}

/** `3.35 / 3.32 V · Δ 30 mV`, with an em dash for anything the pack did not report. */
export function formatCellPair(cells: PackCells | null | undefined): string {
  const { min, max, spread } = readings(cells);
  return `${orNo(max, (v) => v.toFixed(2))} / ${orNo(min, (v) => v.toFixed(2))} V · Δ ${orNo(spread, (v) => Math.round(v).toString())} mV`;
}

/** `pack 49.7 V · 29.1 °C`, with an em dash for anything the pack did not report. */
export function formatPackContext(cells: PackCells | null | undefined): string {
  const { voltage, temp } = readings(cells);
  return `pack ${orNo(voltage, (v) => v.toFixed(1))} V · ${orNo(temp, (v) => v.toFixed(1))} °C`;
}

/** The full reading for tooltips and tests: pair, then pack context. */
export function formatCells(cells: PackCells | null | undefined): string {
  if (missing(cells)) return NO_VALUE;
  return `${formatCellPair(cells)} · ${formatPackContext(cells)}`;
}

/** Age of the same reading that produced the cells, with an honest stale flag. */
export function cellAge(
  sampledAt: string | null | undefined,
  now = Date.now(),
): { label: string; stale: boolean } {
  const at = sampledAt === null || sampledAt === undefined ? NaN : Date.parse(sampledAt);
  if (!Number.isFinite(at)) return { label: NO_VALUE, stale: true };
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  const label =
    seconds === 0 ? 'just now' : seconds < 60 ? `${seconds} s ago` : ago(sampledAt, now);
  return { label, stale: seconds > CELL_STALE_AFTER_S };
}
