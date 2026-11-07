/**
 * Sign conventions, in one place.
 *
 * The contract is explicit: `powerW` positive charges the battery and negative discharges it;
 * a positive grid value is import. Getting this wrong in one component is how a dashboard starts
 * lying, so every screen reads these helpers instead of comparing to zero itself.
 */

export type GridFlow = 'import' | 'export' | 'idle';
export type BatteryFlow = 'charge' | 'discharge' | 'idle';

/** Below this the reading is noise, not a direction (W). */
export const FLOW_DEADBAND_W = 15;

/** Positive grid power is import (the house draws from the grid). */
export function gridFlow(gridW: number | null | undefined): GridFlow {
  if (gridW === null || gridW === undefined || Math.abs(gridW) < FLOW_DEADBAND_W) return 'idle';
  return gridW > 0 ? 'import' : 'export';
}

/** Positive battery power charges the battery, negative discharges it. */
export function batteryFlow(powerW: number | null | undefined): BatteryFlow {
  if (powerW === null || powerW === undefined || Math.abs(powerW) < FLOW_DEADBAND_W) return 'idle';
  return powerW > 0 ? 'charge' : 'discharge';
}

/** The verb a screen shows next to a power reading. */
export function flowVerb(flow: BatteryFlow | GridFlow): string {
  switch (flow) {
    case 'charge':
      return 'charging';
    case 'discharge':
      return 'discharging';
    case 'import':
      return 'importing';
    case 'export':
      return 'exporting';
    default:
      return 'idle';
  }
}

/** Tone class for a state of charge: a low pack is a warning, not a decorative green ring. */
export function socTone(socPct: number | null | undefined): 'good' | 'warn' | 'bad' | 'muted' {
  if (socPct === null || socPct === undefined) return 'muted';
  if (socPct <= 15) return 'bad';
  if (socPct <= 30) return 'warn';
  return 'good';
}

/** Tone for the energy balance: exporting is good, importing is neutral, a big import is a warn. */
export function gridTone(gridW: number | null | undefined): 'good' | 'warn' | 'muted' {
  const flow = gridFlow(gridW);
  if (flow === 'export') return 'good';
  if (gridW !== null && gridW !== undefined && gridW > 3000) return 'warn';
  return 'muted';
}

/** Sign of a saving amount, for the money cards. */
export function moneyTone(value: number | null | undefined): 'good' | 'bad' | 'muted' {
  if (value === null || value === undefined || value === 0) return 'muted';
  return value > 0 ? 'good' : 'bad';
}
