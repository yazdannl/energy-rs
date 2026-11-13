/**
 * Deterministic fixtures for `?mock=1`.
 *
 * The shapes are the contract's; the numbers are a plausible 2026 Zonneplan household: a 4 kWp
 * roof, two 1.92 kWh packs, negative midday prices and a saldering regime that still nets 1:1.
 * Nothing here is random, so the UI (and a screenshot) is reproducible.
 */

import type {
  DispatchResponse,
  Health,
  Mode,
  Pack,
  PacksResponse,
  Plan,
  PlanSlot,
  PriceSlot,
  PricesResponse,
  Savings,
  SavingsDay,
  SavingsPeriod,
  SettingsResponse,
  Site,
  LoadsResponse,
  StreamState,
} from '../types';

const QUARTER_MS = 900_000;
const CAPACITY_WH = 3840;
const MAX_POWER_W = 1600;
const MIN_SOC_PCT = 12;
const MAX_SOC_PCT = 97;
const ENERGY_TAX_EX = 0.09161;
const FEE_INCL = 0.02;
const VAT = 0.21;

export const MOCK_MODE: Mode = 'dry_run';

export function iso(date: Date): string {
  return date.toISOString().replace(/\.\d{3}Z$/, 'Z');
}

export function floorQuarter(date: Date): Date {
  return new Date(Math.floor(date.getTime() / QUARTER_MS) * QUARTER_MS);
}

export function round6(value: number): number {
  return Math.round(value * 1e6) / 1e6;
}

function localHour(date: Date): number {
  return date.getHours() + date.getMinutes() / 60;
}

/** Market price EUR/kWh: evening peak, morning shoulder, midday dip (occasionally negative). */
function marketAt(date: Date): number {
  const hour = localHour(date);
  const evening = Math.exp(-((hour - 19.5) ** 2) / 8) * 0.16;
  const morning = Math.exp(-((hour - 8) ** 2) / 6) * 0.09;
  const midday = Math.exp(-((hour - 12.5) ** 2) / 6) * 0.14;
  return round6(0.055 + evening + morning - midday);
}

function buyAllIn(market: number): number {
  return round6((market + ENERGY_TAX_EX) * (1 + VAT) + FEE_INCL);
}

/** PV production for a slot, Wh (bell curve, zero before 07:00 and after 20:00). */
function pvWhAt(date: Date): number {
  const hour = localHour(date);
  const width = 4.6;
  const bell = Math.exp(-((hour - 13.2) ** 2) / (2 * width * width));
  return Math.round(bell * 2100 * 0.25 * 10) / 10;
}

/** Household load for a slot, Wh: a base plus morning and evening peaks. */
function loadWhAt(date: Date): number {
  const hour = localHour(date);
  const evening = Math.exp(-((hour - 19) ** 2) / 6) * 380;
  const morning = Math.exp(-((hour - 7.5) ** 2) / 5) * 220;
  return Math.round((140 + evening + morning) * 0.25 * 10) / 10;
}

export function makePrices(now: Date, count = 192): PricesResponse {
  const from = floorQuarter(now);
  const to = new Date(from.getTime() + count * QUARTER_MS);
  const slots: PriceSlot[] = [];
  for (let i = 0; i < count; i += 1) {
    const start = new Date(from.getTime() + i * QUARTER_MS);
    const market = marketAt(start);
    const buy = buyAllIn(market);
    slots.push({
      startAt: iso(start),
      endAt: iso(new Date(start.getTime() + QUARTER_MS)),
      buyEurPerKwh: buy,
      marketEurPerKwh: market,
      // 2026: saldering still nets 1:1, so an exported kWh earns the all-in price.
      feedInEurPerKwh: Math.max(buy, 0),
      source: 'zonneplan_quarter_hourly',
    });
  }
  return { from: iso(from), to: iso(to), currency: 'EUR', count, slots };
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

/**
 * A 30 h plan solved the way the engine does it: import when the slot is cheap, discharge when it
 * is dear. PV surplus is *exported* rather than stored, because in 2026 saldering still nets an
 * exported kWh 1:1 — storing it would lose money, which is exactly what the engine would not do.
 */
export function makePlan(now: Date): Plan {
  const from = floorQuarter(now);
  const prices = makePrices(from, 120);
  const reference = median(prices.slots.map((slot) => slot.buyEurPerKwh));
  const low = reference * 0.9;
  const high = reference * 1.1;

  let socWh = CAPACITY_WH * 0.45;
  let costEur = 0;
  let baselineCostEur = 0;
  let chargedWh = 0;
  let dischargedWh = 0;
  let gridChargeWh = 0;
  const reasons: Record<string, number> = {};

  const slots: PlanSlot[] = prices.slots.map((price, idx) => {
    const start = new Date(price.startAt);
    const pvWh = pvWhAt(start);
    const loadWh = loadWhAt(start);
    const minWh = (CAPACITY_WH * MIN_SOC_PCT) / 100;
    const maxWh = (CAPACITY_WH * MAX_SOC_PCT) / 100;

    let powerW = 0;
    let reason = 'idle';
    if (price.buyEurPerKwh <= low && socWh < maxWh - 20) {
      powerW = MAX_POWER_W;
      reason = 'price_low';
    } else if (price.buyEurPerKwh >= high && socWh > minWh + 120) {
      powerW = -MAX_POWER_W;
      reason = 'price_high';
    }

    const wanted = (powerW * 900) / 3600;
    const after = Math.min(maxWh, Math.max(minWh, socWh + wanted));
    powerW = Math.round(((after - socWh) * 3600) / 900);

    const batteryWh = after - socWh;
    if (batteryWh > 0) {
      chargedWh += batteryWh;
      gridChargeWh += batteryWh;
    }
    if (batteryWh < 0) dischargedWh -= batteryWh;
    socWh = after;
    reasons[reason] = (reasons[reason] ?? 0) + 1;

    // Positive battery power is extra consumption (charging); negative power serves the house.
    const gridWh = loadWh - pvWh + batteryWh;
    const priceForSlot = gridWh >= 0 ? price.buyEurPerKwh : price.feedInEurPerKwh;
    costEur += (gridWh / 1000) * priceForSlot;
    const baselineGridWh = loadWh - pvWh;
    baselineCostEur +=
      (baselineGridWh / 1000) * (baselineGridWh >= 0 ? price.buyEurPerKwh : price.feedInEurPerKwh);

    return {
      idx,
      startAt: price.startAt,
      endAt: price.endAt,
      buyEurPerKwh: price.buyEurPerKwh,
      feedInEurPerKwh: price.feedInEurPerKwh,
      pvWh: roundValue(pvWh),
      loadWh: roundValue(loadWh),
      batteryPowerW: roundValue(powerW),
      socWh: roundValue(after),
      targetSocWh: Math.round(after),
      reason,
    };
  });

  const bindingReasons = Object.entries(reasons)
    .filter(([reason]) => reason !== 'idle')
    .sort((left, right) => right[1] - left[1])
    .slice(0, 2)
    .map(([reason]) => reason);

  const horizonEnd = new Date(from.getTime() + 120 * QUARTER_MS);
  return {
    planId: 41,
    createdAt: iso(new Date(from.getTime() - 90_000)),
    mode: MOCK_MODE,
    horizonStartAt: iso(from),
    horizonEndAt: iso(horizonEnd),
    feasible: true,
    summary: {
      costEur: round6(costEur),
      baselineCostEur: round6(baselineCostEur),
      savedEur: round6(baselineCostEur - costEur),
      chargedWh: roundValue(chargedWh),
      dischargedWh: roundValue(dischargedWh),
      gridChargeWh: roundValue(gridChargeWh),
      pvChargeWh: 0,
      cycles: round6(chargedWh / CAPACITY_WH),
      terminalTargetWh: null,
      infeasibleReason: null,
      exportFloorEurPerKwh: null,
      zeroExport: false,
      costBasisEurPerKwh: null,
      peakReserveWh: null,
      solarReserveWh: 0,
      clippedWh: 0,
      gatedWh: 0,
    },
    bindingReasons: bindingReasons.length === 0 ? ['pv_surplus'] : bindingReasons,
    slots,
  };
}

function roundValue(value: number): number {
  return Math.round(value * 100) / 100;
}

export function makePacks(now: Date): PacksResponse {
  const packs: Pack[] = [
    {
      sn: 'AB2000-A1',
      name: 'Attic A',
      alias: null,
      labelCapacityKwh: 1.92,
      reportedCapacityKwh: 1.88,
      limits: { maxChargeW: 800, maxDischargeW: 800 },
      firstSeenAt: iso(new Date(now.getTime() - 92 * 86_400_000)),
      lastSeenAt: iso(new Date(now.getTime() - 4_000)),
      state: {
        socPct: 62.4,
        powerW: 412,
        sampledAt: iso(new Date(now.getTime() - 4_000)),
        stale: false,
      },
      learned: {
        capacityKwh: 1.85,
        capacityConfidence: 0.82,
        capacityApplied: true,
        capacityDecision: 'capacity_learned_full_cycle',
        capacityBandLowKwh: 1.72,
        capacityBandHighKwh: 1.92,
        roundTrip: 0.9,
        etaCharge: 0.9486833,
        etaDischarge: 0.9486833,
        efficiencyConfidence: 0.71,
        efficiencyApplied: true,
        sessionsUsed: 6,
        updatedAt: iso(new Date(now.getTime() - 3 * 86_400_000)),
      },
      latch: {
        blocked: false,
        forceCharge: false,
        forceChargeW: null,
        since: null,
        reason: null,
        reasonAt: null,
        voltageV: 3.32,
        lastFreshAt: iso(new Date(now.getTime() - 4_000)),
      },
      override: null,
    },
    {
      sn: 'AB2000-B2',
      name: 'Attic B',
      alias: null,
      labelCapacityKwh: 1.92,
      reportedCapacityKwh: null,
      limits: { maxChargeW: 800, maxDischargeW: 800 },
      firstSeenAt: iso(new Date(now.getTime() - 41 * 86_400_000)),
      lastSeenAt: iso(new Date(now.getTime() - 62_000)),
      state: {
        socPct: 18.5,
        powerW: -120,
        sampledAt: iso(new Date(now.getTime() - 62_000)),
        stale: true,
      },
      learned: {
        capacityKwh: 1.91,
        capacityConfidence: 0.34,
        capacityApplied: false,
        capacityDecision: 'capacity_inconclusive',
        capacityBandLowKwh: 1.55,
        capacityBandHighKwh: 1.98,
        roundTrip: null,
        etaCharge: null,
        etaDischarge: null,
        efficiencyConfidence: 0.22,
        efficiencyApplied: false,
        sessionsUsed: 1,
        updatedAt: iso(new Date(now.getTime() - 9 * 86_400_000)),
      },
      latch: {
        blocked: true,
        forceCharge: true,
        forceChargeW: 300,
        since: iso(new Date(now.getTime() - 5_400_000)),
        reason: 'cell_below_resume',
        reasonAt: iso(new Date(now.getTime() - 5_400_000)),
        voltageV: 3.01,
        lastFreshAt: iso(new Date(now.getTime() - 62_000)),
      },
      override: {
        action: 'hold',
        powerW: null,
        expiresAt: iso(new Date(now.getTime() + 11 * 60_000)),
      },
    },
  ];
  return { packs };
}

export function makeSite(now: Date): Site {
  return {
    configured: true,
    mode: MOCK_MODE,
    lastTickAt: iso(new Date(now.getTime() - 20_000)),
    capabilities: ['battery_control', 'savings', 'forecast'],
    sources: [
      { kind: 'grid', entityId: 'sensor.p1_meter_power', label: 'Grid' },
      { kind: 'solar', entityId: 'sensor.inverter_power', label: 'Roof PV' },
      { kind: 'load', entityId: 'sensor.house_load', label: 'House' },
      { kind: 'battery', entityId: 'sensor.battery_power', label: 'Battery' },
    ],
    packs: makePacks(now).packs.map((pack) => ({
      sn: pack.sn,
      name: pack.name,
      alias: pack.alias,
      labelCapacityKwh: pack.labelCapacityKwh,
      reportedCapacityKwh: pack.reportedCapacityKwh,
      maxChargeW: pack.limits.maxChargeW,
      maxDischargeW: pack.limits.maxDischargeW,
      firstSeenAt: pack.firstSeenAt,
      lastSeenAt: pack.lastSeenAt,
    })),
  };
}

function dayKey(date: Date): string {
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 10);
}

function makeDay(now: Date, offset: number): SavingsDay {
  const date = new Date(now.getTime() - offset * 86_400_000);
  const wave = Math.sin((offset + 2) * 0.9) * 0.28 + 0.72;
  const pvWh = Math.round(9800 * wave);
  const importWh = Math.round(4200 * (1.3 - wave * 0.6));
  const exportWh = Math.round(pvWh * 0.32);
  const baselineEur = round6((importWh / 1000) * 0.268);
  const actualEur = round6(baselineEur * (0.55 + 0.12 * (1 - wave)));
  const savingEur = round6(baselineEur - actualEur);

  // The four components sum to the saving exactly, as the API promises.
  const arbitrageEur = round6(savingEur * 0.45);
  const selfConsumptionEur = round6(savingEur * 0.4);
  const feedInEur = round6(savingEur * 0.22);
  const batteryBalanceEur = round6(savingEur - arbitrageEur - selfConsumptionEur - feedInEur);

  const provisional = offset === 0;
  const gapped = offset === 3;
  const moneyPresent = !gapped;

  return {
    day: dayKey(date),
    actualEur: moneyPresent ? actualEur : null,
    baselineEur: moneyPresent ? baselineEur : null,
    savingEur: moneyPresent ? savingEur : null,
    arbitrageEur: moneyPresent ? arbitrageEur : null,
    selfConsumptionEur: moneyPresent ? selfConsumptionEur : null,
    feedInEur: moneyPresent ? feedInEur : null,
    batteryBalanceEur: moneyPresent ? batteryBalanceEur : null,
    importWh,
    exportWh,
    pvWh,
    batteryChargeWh: Math.round(pvWh * 0.42),
    batteryDischargeWh: Math.round(pvWh * 0.34),
    coverageHours: provisional ? 14.5 : gapped ? 9.5 : 24,
    complete: !provisional && !gapped,
  };
}

export function makeSavings(now: Date, period: SavingsPeriod): Savings {
  const counts: Record<SavingsPeriod, number> = { day: 1, month: 19, year: 118, lifetime: 402 };
  const days = Array.from({ length: counts[period] }, (_, offset) =>
    makeDay(now, offset),
  ).reverse();
  // `lifetime` sums complete days only; shorter periods also count the provisional current day.
  // A day with `null` money is in `days` but in no total.
  const counted = days.filter(
    (day) => day.savingEur !== null && (period !== 'lifetime' || day.complete),
  );
  const totals = counted.reduce(
    (acc, day) => ({
      actualEur: acc.actualEur + (day.actualEur ?? 0),
      baselineEur: acc.baselineEur + (day.baselineEur ?? 0),
      savingEur: acc.savingEur + (day.savingEur ?? 0),
      arbitrageEur: acc.arbitrageEur + (day.arbitrageEur ?? 0),
      selfConsumptionEur: acc.selfConsumptionEur + (day.selfConsumptionEur ?? 0),
      feedInEur: acc.feedInEur + (day.feedInEur ?? 0),
      batteryBalanceEur: acc.batteryBalanceEur + (day.batteryBalanceEur ?? 0),
      importWh: acc.importWh + day.importWh,
      exportWh: acc.exportWh + day.exportWh,
      pvWh: acc.pvWh + day.pvWh,
      batteryChargeWh: acc.batteryChargeWh + day.batteryChargeWh,
      batteryDischargeWh: acc.batteryDischargeWh + day.batteryDischargeWh,
    }),
    {
      actualEur: 0,
      baselineEur: 0,
      savingEur: 0,
      arbitrageEur: 0,
      selfConsumptionEur: 0,
      feedInEur: 0,
      batteryBalanceEur: 0,
      importWh: 0,
      exportWh: 0,
      pvWh: 0,
      batteryChargeWh: 0,
      batteryDischargeWh: 0,
    },
  );
  return {
    period,
    fromDay: days[0]?.day ?? dayKey(now),
    toDay: dayKey(now),
    days,
    totals: {
      actualEur: round6(totals.actualEur),
      baselineEur: round6(totals.baselineEur),
      savingEur: round6(totals.savingEur),
      arbitrageEur: round6(totals.arbitrageEur),
      selfConsumptionEur: round6(totals.selfConsumptionEur),
      feedInEur: round6(totals.feedInEur),
      batteryBalanceEur: round6(totals.batteryBalanceEur),
      importWh: Math.round(totals.importWh),
      exportWh: Math.round(totals.exportWh),
      pvWh: Math.round(totals.pvWh),
      batteryChargeWh: Math.round(totals.batteryChargeWh),
      batteryDischargeWh: Math.round(totals.batteryDischargeWh),
      days: days.length,
      completeDays: days.filter((day) => day.complete).length,
    },
  };
}

export function makeSettings(now: Date): SettingsResponse {
  return {
    asOf: iso(now),
    values: {
      grid_import_limit_w: 0,
      net_metering: 'on',
      zero_export: 'off',
      solar_reserve_pct: 20,
      peak_reserve_pct: 20,
      peak_reserve_value_eur_per_kwh: 0.15,
      wear_eur_per_kwh: 0.02,
      cost_basis_eur_per_kwh: 0,
      export_policy: 'legacy',
      grid_export_limit_w: 0,
      reserve_soc_pct: 12,
      max_soc_pct: 97,
      battery_power_limit_w: 1600,
      price_provider: 'zonneplan',
      price_entity: 'sensor.zonneplan_current_electricity_tariff',
      replan_seconds: 300,
      stale_input_seconds: 180,
      full_charge_weekday: 'sunday',
      full_charge_time: '02:30',
      control_mode: 'dry_run',
    },
    schema: [
      { key: 'grid_import_limit_w', kind: 'number', default: 0, min: 0, max: 20000 },
      {
        key: 'net_metering',
        kind: 'choice',
        default: 'on',
        options: ['on', 'off'],
      },
      { key: 'zero_export', kind: 'choice', default: 'off', options: ['off', 'on'] },
      { key: 'grid_export_limit_w', kind: 'number', default: 0, min: 0, max: 20000 },
      { key: 'reserve_soc_pct', kind: 'number', default: 10, min: 0, max: 60 },
      { key: 'max_soc_pct', kind: 'number', default: 100, min: 50, max: 100 },
      { key: 'battery_power_limit_w', kind: 'number', default: 1600, min: 100, max: 12000 },
      {
        key: 'price_provider',
        kind: 'choice',
        default: 'zonneplan',
        options: ['zonneplan', 'ha'],
      },
      { key: 'price_entity', kind: 'text', default: '' },
      { key: 'replan_seconds', kind: 'number', default: 300, min: 60, max: 86400 },
      { key: 'stale_input_seconds', kind: 'number', default: 180, min: 30, max: 3600 },
      {
        key: 'full_charge_weekday',
        kind: 'choice',
        default: 'sunday',
        options: ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'],
      },
      { key: 'full_charge_time', kind: 'text', default: '02:00' },
    ],
  };
}

export function makeDispatch(now: Date): DispatchResponse {
  const entries = [0, 1, 2, 3, 4, 5].map((index) => {
    const at = new Date(now.getTime() - (index + 1) * 900_000);
    const requestedW = index % 3 === 0 ? 800 : index % 3 === 1 ? -400 : 0;
    return {
      at: iso(at),
      packSn: index % 2 === 0 ? 'AB2000-A1' : 'AB2000-B2',
      requestedW,
      appliedW: index === 4 ? 0 : requestedW,
      reason: index === 4 ? 'stale_input' : index === 5 ? 'hold' : 'dispatch',
      mode: MOCK_MODE,
      verified: index !== 4,
      error: index === 4 ? 'input stale by 212 s' : null,
    };
  });
  return { windowSeconds: 604_800, count: entries.length, entries };
}

export function makeHealth(now: Date): Health {
  return {
    status: 'ok',
    version: '0.1.0-mock',
    mode: MOCK_MODE,
    lastTickAt: iso(new Date(now.getTime() - 20_000)),
  };
}

/** One live tick: meter balanced as `grid = load - pv + battery`, SoC drifting slowly. */
export function makeStreamState(now: Date, tick: number): StreamState {
  const phase = (tick % 240) / 240;
  const pvW = Math.max(0, Math.round(2380 * Math.exp(-((localHour(now) - 13.1) ** 2) / 44)));
  const loadW = Math.round(520 + 180 * Math.sin(phase * Math.PI * 2));
  const batteryW = tick % 7 < 4 ? 760 : -420;
  const soc = Math.min(96, Math.max(14, 62.4 + phase * 6 - (tick % 60) * 0.02));
  return {
    mode: MOCK_MODE,
    lastTickAt: iso(now),
    planId: 41,
    packs: [
      {
        sn: 'AB2000-A1',
        socPct: Math.round(soc * 10) / 10,
        powerW: batteryW,
        stale: false,
        blocked: false,
      },
      {
        sn: 'AB2000-B2',
        socPct: Math.round((soc - 43) * 10) / 10,
        powerW: batteryW > 0 ? 0 : -90,
        stale: true,
        blocked: true,
      },
    ],
    meter: {
      at: iso(now),
      gridW: loadW - pvW + batteryW,
      pvW,
      loadW,
      batteryW,
    },
    forecast: {
      at: iso(now),
      pvWh: Math.round(pvW * 0.9),
      loadWh: Math.round(loadW * 1.1),
      confidence: 0.78,
    },
    faults: [
      {
        kind: 'stale_input',
        message: 'AB2000-B2 has not reported for 64 s; the pack is held.',
        since: iso(new Date(now.getTime() - 5_400_000)),
      },
    ],
  };
}

/** The load-prediction settings the mock serves: one heat pump that runs while away. */
export function makeLoads(now: Date, holiday = false): LoadsResponse {
  return {
    holiday,
    temperatureEntity: 'sensor.outdoor_temperature',
    weatherEntity: 'weather.home',
    devices: [
      {
        entity: 'sensor.heat_pump_power',
        kind: 'heat_pump',
        name: 'Heat pump',
        keepWhileAway: true,
      },
    ],
    problems: [],
    profile: {
      days: 34,
      samples: 812,
      baseKw: 0.18,
      houseBaseKw: 0.24,
      updatedAt: iso(new Date(now.getTime() - 3 * 3_600_000)),
      temperatureHours: 640,
      accounting: {
        subtracted: ['device:sensor.heat_pump_power'],
        excluded: [],
        droppedHours: 0,
        clampedHours: 0,
      },
    },
    modelled: [{ entity: 'sensor.heat_pump_power', modelled: true }],
  };
}
