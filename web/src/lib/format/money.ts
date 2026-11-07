/**
 * Money formatting.
 *
 * The ledger resolves money to 1e-6 EUR and the API rounds to 6 decimals, so the exact view uses
 * six digits and the everyday view uses two. Nothing here re-rounds an amount before displaying
 * it: the value shown is the value the API sent.
 */

import { NO_VALUE, orNo } from './empty';

const eur = new Intl.NumberFormat('nl-NL', {
  style: 'currency',
  currency: 'EUR',
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

const eurExact = new Intl.NumberFormat('nl-NL', {
  style: 'currency',
  currency: 'EUR',
  minimumFractionDigits: 6,
  maximumFractionDigits: 6,
});

const eurPerKwh = new Intl.NumberFormat('nl-NL', {
  style: 'currency',
  currency: 'EUR',
  minimumFractionDigits: 3,
  maximumFractionDigits: 3,
});

/** Two decimals, the amount a household reads. */
export function fmtEur(value: number | null | undefined): string {
  return orNo(value, (present) => eur.format(present));
}

/** Six decimals, the ledger's resolution (provenance, reconciliation, tooltips). */
export function fmtEur6(value: number | null | undefined): string {
  return orNo(value, (present) => eurExact.format(present));
}

/** Per-kWh prices need the third decimal: a slot can be 0.0015 apart. */
export function fmtEurPerKwh(value: number | null | undefined): string {
  return orNo(value, (present) => `${eurPerKwh.format(present)}/kWh`);
}

/** Round to the API's 6-decimal resolution; used for client-side sums of API values. */
export function round6(value: number): number {
  return Math.round(value * 1e6) / 1e6;
}

export { NO_VALUE };
