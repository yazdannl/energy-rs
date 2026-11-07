/**
 * The settlement preview: the Rust contract's formula, mirrored for the Prices screen.
 *
 * Source of truth is `crates/energy-price/src/settlement/{contract,net_metering}.rs`. Only the two
 * things the preview needs are mirrored — the post-saldering feed-in price and whether 1:1 netting
 * applies — and every constant carries the file it comes from so a rate change has one place to
 * land. This is a *preview*, not an accounting: `feedInEurPerKwh` from the API stays authoritative.
 */

import { round6 } from './money';

/** `contract.rs`: VAT on energy. */
export const VAT_NL = 0.21;
/** `contract.rs`: Zonneplan inkoopvergoeding, EUR/kWh incl. VAT. */
export const FEE_INCL = 0.02;
/** `contract.rs`: fee Zonneplan pays on exported kWh, EUR/kWh. */
export const FEED_IN_FEE = 0.02;
/** `contract.rs`: terugleverkosten, EUR/kWh (Zonneplan: none). */
export const TERUGLEVER_KOSTEN = 0;
/** `bonus.rs`: Zonnebonus 10 %, PV-only, daylight-only, 7 500 kWh/yr. */
export const BONUS_PCT = 0.1;
export const BONUS_CAP_KWH = 7500;
/** `tax_year.rs`: energiebelasting 2026, EUR/kWh excl. VAT. */
export const ENERGY_TAX_2026_EX = 0.09161;
/** `net_metering.rs`: the first local year without saldering. */
export const SALDERING_END_YEAR = 2027;

/** Date-driven regime: 1:1 netting while the local tariff year is before 2027. */
export function netMeteringActive(when: Date): boolean {
  return when.getFullYear() < SALDERING_END_YEAR;
}

/** `feed_in_post = (market + feed_in_fee) * bonusMultiplier - teruglever_kosten`, unclamped. */
export function feedInPost(market: number, bonusApplies = false): number {
  const multiplier = bonusApplies ? 1 + BONUS_PCT : 1;
  return round6((market + FEED_IN_FEE) * multiplier - TERUGLEVER_KOSTEN);
}

/**
 * The export price for a slot: all-in while netting applies, else the post-saldering feed-in,
 * clamped at zero (export is refused, never charged).
 */
export function exportPrice(
  slot: { buyEurPerKwh: number; marketEurPerKwh: number },
  nettingOn: boolean,
  bonusApplies = false,
): number {
  if (nettingOn) return round6(Math.max(slot.buyEurPerKwh, 0));
  return round6(Math.max(feedInPost(slot.marketEurPerKwh, bonusApplies), 0));
}

/** What the date-driven regime means for the household, in one sentence. */
export function salderingHint(now: Date): string {
  const year = now.getFullYear();
  if (year < SALDERING_END_YEAR) {
    return `Date-driven: 1:1 netting applies through ${SALDERING_END_YEAR - 1}, so an exported kWh earns the all-in import price.`;
  }
  return `Date-driven: netting ended on ${SALDERING_END_YEAR}-01-01, so export earns (market + €${FEED_IN_FEE.toFixed(2)}) per kWh.`;
}
