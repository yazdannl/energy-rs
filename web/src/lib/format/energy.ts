/**
 * Energy and power formatting.
 *
 * Units live in the field names (`powerW`, `energyWh`), so the formatter never guesses: watts are
 * watts until 1 kW, and Wh becomes kWh above 1000 Wh with at most two decimals.
 */

import { orNo } from './empty';

const decimal = new Intl.NumberFormat('nl-NL', { maximumFractionDigits: 2 });
const integer = new Intl.NumberFormat('nl-NL', { maximumFractionDigits: 0 });

/** `250 Wh`, `2.5 kWh`, `-1.2 kWh` (the sign is the caller's convention). */
export function fmtWh(value: number | null | undefined): string {
  return orNo(value, (wh) => {
    if (Math.abs(wh) < 1000) return `${integer.format(wh)} Wh`;
    return `${decimal.format(wh / 1000)} kWh`;
  });
}

/** kWh from a field that already stores kWh (`capacityKwh`). */
export function fmtKwh(value: number | null | undefined): string {
  return orNo(value, (kwh) => `${decimal.format(kwh)} kWh`);
}

/** `640 W`, `1.24 kW`; the sign is kept as sent (positive charges). */
export function fmtW(value: number | null | undefined): string {
  return orNo(value, (watt) => {
    if (Math.abs(watt) < 1000) return `${integer.format(watt)} W`;
    return `${decimal.format(watt / 1000)} kW`;
  });
}

/** Percentage with at most one decimal, e.g. `62.4 %`. */
export function fmtPct(value: number | null | undefined): string {
  return orNo(
    value,
    (pct) => `${new Intl.NumberFormat('nl-NL', { maximumFractionDigits: 1 }).format(pct)} %`,
  );
}

/** A 0..1 confidence as a percentage (`0.82` → `82 %`). */
export function fmtConfidence(value: number | null | undefined): string {
  return orNo(value, (ratio) => `${integer.format(ratio * 100)} %`);
}
