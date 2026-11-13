/**
 * The wire contract of `docs/API.md` (v1), transcribed field by field.
 *
 * Names are exactly what the server emits: `camelCase`, units in the field name, money in EUR
 * (6 decimals), energy in Wh, power in W (positive charges the battery). `null` is a real answer
 * from the server, not an absence of an answer, so it stays in the type instead of becoming
 * `undefined`.
 */

export type Mode = 'advisory' | 'dry_run' | 'auto';
export type OverrideAction = 'charge' | 'hold' | 'off';
export type SavingsPeriod = 'day' | 'month' | 'year' | 'lifetime';
export type SettingKind = 'number' | 'choice' | 'text';
export type SettingValue = number | string | boolean | null;
export type PriceSource =
  'zonneplan_quarter_hourly' | 'zonneplan_hourly' | 'ha_quarter_hourly' | 'ha_hourly';

/** An RFC3339 UTC instant with second resolution, e.g. `2026-01-01T00:00:00Z`. */
export type Instant = string;
/** A `Europe/Amsterdam` local day, `YYYY-MM-DD`. */
export type LocalDay = string;

export interface Health {
  status: string;
  version: string;
  mode: Mode;
  lastTickAt: Instant | null;
}

export interface SiteSource {
  kind: string;
  entityId: string;
  label: string | null;
}

export interface SitePack {
  sn: string;
  name: string | null;
  alias: string | null;
  labelCapacityKwh: number | null;
  reportedCapacityKwh: number | null;
  maxChargeW: number | null;
  maxDischargeW: number | null;
  firstSeenAt: Instant | null;
  lastSeenAt: Instant | null;
}

export interface Site {
  configured: boolean;
  mode: Mode;
  lastTickAt: Instant | null;
  capabilities: string[];
  sources: SiteSource[];
  packs: SitePack[];
}

export interface PackLimits {
  maxChargeW: number | null;
  maxDischargeW: number | null;
}

export interface PackState {
  socPct: number | null;
  powerW: number | null;
  sampledAt: Instant | null;
  stale: boolean;
}

export interface PackLearned {
  capacityKwh: number | null;
  capacityConfidence: number | null;
  capacityApplied: boolean;
  capacityDecision: string | null;
  capacityBandLowKwh: number | null;
  capacityBandHighKwh: number | null;
  roundTrip: number | null;
  etaCharge: number | null;
  etaDischarge: number | null;
  efficiencyConfidence: number | null;
  efficiencyApplied: boolean;
  sessionsUsed: number | null;
  updatedAt: Instant | null;
}

export interface PackLatch {
  blocked: boolean;
  forceCharge: boolean;
  forceChargeW: number | null;
  since: Instant | null;
  reason: string | null;
  reasonAt: Instant | null;
  voltageV: number | null;
  lastFreshAt: Instant | null;
}

export interface PackOverride {
  action: OverrideAction;
  powerW: number | null;
  expiresAt: Instant | null;
}

export interface Pack {
  sn: string;
  name: string | null;
  alias: string | null;
  labelCapacityKwh: number | null;
  reportedCapacityKwh: number | null;
  limits: PackLimits;
  firstSeenAt: Instant | null;
  lastSeenAt: Instant | null;
  state: PackState | null;
  learned: PackLearned | null;
  latch: PackLatch | null;
  override: PackOverride | null;
}

export interface PacksResponse {
  packs: Pack[];
}

/** The pack identity after `PATCH /packs/{sn}`: `alias` is `null` when the label was cleared. */
export interface PackAlias {
  sn: string;
  alias: string | null;
}

export interface PriceSlot {
  startAt: Instant;
  endAt: Instant;
  buyEurPerKwh: number;
  marketEurPerKwh: number;
  feedInEurPerKwh: number;
  source: PriceSource;
}

export interface PricesResponse {
  from: Instant;
  to: Instant;
  currency: string;
  count: number;
  slots: PriceSlot[];
}

export interface PlanSummary {
  costEur: number;
  baselineCostEur: number;
  savedEur: number;
  chargedWh: number;
  dischargedWh: number;
  gridChargeWh: number;
  pvChargeWh: number;
  cycles: number;
  terminalTargetWh: number | null;
  infeasibleReason: string | null;
  /** The break-even an exported kWh had to clear, EUR/kWh. */
  exportFloorEurPerKwh: number | null;
  /** Whether export is disabled outright. */
  zeroExport: boolean;
  /** The cost basis the export gate was armed with, EUR/kWh. */
  costBasisEurPerKwh: number | null;
  /** The soft peak guideline in force, Wh. */
  peakReserveWh: number | null;
  /** Headroom kept free of grid energy, Wh. */
  solarReserveWh: number;
  /** PV a ceiling forced out of the pack, Wh. */
  clippedWh: number;
  /** Battery energy the export gate kept in the pack, Wh. */
  gatedWh: number;
}

export interface PlanSlot {
  idx: number;
  startAt: Instant;
  endAt: Instant;
  buyEurPerKwh: number;
  feedInEurPerKwh: number;
  pvWh: number;
  loadWh: number;
  batteryPowerW: number;
  socWh: number;
  targetSocWh: number;
  reason: string | null;
}

export interface Plan {
  planId: number;
  createdAt: Instant;
  mode: Mode;
  horizonStartAt: Instant;
  horizonEndAt: Instant;
  feasible: boolean;
  summary: PlanSummary;
  bindingReasons: string[];
  slots: PlanSlot[];
}

export interface PlanRow {
  planId: number;
  createdAt: Instant;
  mode: Mode;
  horizonStartAt: Instant;
  horizonEndAt: Instant;
  costEur: number | null;
  baselineCostEur: number | null;
  savedEur: number | null;
  feasible: boolean | null;
  infeasibleReason: string | null;
}

export interface PlansResponse {
  count: number;
  plans: PlanRow[];
}

export interface DispatchEntry {
  at: Instant;
  packSn: string;
  requestedW: number;
  appliedW: number;
  reason: string;
  mode: Mode;
  verified: boolean;
  error: string | null;
}

export interface DispatchResponse {
  windowSeconds: number;
  count: number;
  entries: DispatchEntry[];
}

/**
 * Money is `null` for a **closed** day whose slots were not all measured: the day keeps its energy
 * sums and `coverageHours` instead of a fabricated number. The current day is provisional — money
 * present, `complete: false` — until it closes.
 */
export interface SavingsDay {
  day: LocalDay;
  actualEur: number | null;
  baselineEur: number | null;
  savingEur: number | null;
  arbitrageEur: number | null;
  selfConsumptionEur: number | null;
  feedInEur: number | null;
  batteryBalanceEur: number | null;
  importWh: number;
  exportWh: number;
  pvWh: number;
  batteryChargeWh: number;
  batteryDischargeWh: number;
  coverageHours: number;
  complete: boolean;
}

/** Totals never carry `null`: a `null` day is in `days` but in no total. */
export interface SavingsTotals {
  actualEur: number;
  baselineEur: number;
  savingEur: number;
  arbitrageEur: number;
  selfConsumptionEur: number;
  feedInEur: number;
  batteryBalanceEur: number;
  importWh: number;
  exportWh: number;
  pvWh: number;
  batteryChargeWh: number;
  batteryDischargeWh: number;
  days: number;
  completeDays: number;
}

export interface Savings {
  period: SavingsPeriod;
  fromDay: LocalDay;
  toDay: LocalDay;
  days: SavingsDay[];
  totals: SavingsTotals;
}

export interface SettingSchemaEntry {
  key: string;
  kind: SettingKind;
  default: SettingValue;
  min?: number;
  max?: number;
  options?: string[];
}

export interface SettingsResponse {
  asOf: Instant;
  values: Record<string, SettingValue>;
  schema: SettingSchemaEntry[];
}

export interface ModeResponse {
  mode: Mode;
  updatedAt: Instant;
  engineNotified: boolean;
}

export interface OverrideResponse {
  accepted: boolean;
  pack: string;
  action: OverrideAction;
  powerW: number | null;
  ttlS: number;
  expiresAt: Instant;
}

export interface ProtectionResponse {
  accepted: boolean;
  action: 'full_charge';
  pack: string | null;
}

/** `POST /replan` — the engine will solve a fresh plan before its next tick. */
export interface ReplanResponse {
  accepted: boolean;
}

export interface LearningResponse {
  accepted: boolean;
  action: 'relearn';
  pack: string | null;
}

/** One pack inside an SSE `state` event. */
export interface LivePack {
  sn: string;
  socPct: number | null;
  powerW: number | null;
  stale: boolean;
  blocked: boolean;
}

export interface LiveMeter {
  at: Instant | null;
  gridW: number;
  pvW: number;
  loadW: number;
  batteryW: number | null;
}

export interface LiveForecast {
  at: Instant | null;
  pvWh: number;
  loadWh: number;
  confidence: number | null;
}

export interface LiveFault {
  kind: string;
  message: string;
  since: Instant | null;
}

export interface StreamState {
  mode: Mode;
  lastTickAt: Instant | null;
  planId: number | null;
  packs: LivePack[];
  meter: LiveMeter | null;
  forecast: LiveForecast | null;
  faults: LiveFault[];
}

export interface PlanEvent {
  planId: number | null;
  at: Instant;
}

export type LinkStatus = 'connecting' | 'live' | 'reconnecting' | 'offline';

/** One configured named load (`load.devices[]`). */
export interface LoadDevice {
  entity: string;
  kind: string;
  name: string;
  keepWhileAway: boolean;
}

/** What the base profile did with the named loads when it was learned. */
export interface LoadAccounting {
  subtracted: string[];
  excluded: { key: string; reason: string }[];
  droppedHours: number;
  clampedHours: number;
}

/** The learned base profile the planner is using, when one is fresh enough. */
export interface LearnedLoadProfile {
  days: number;
  samples: number;
  baseKw: number;
  houseBaseKw: number;
  updatedAt: Instant;
  temperatureHours: number;
  accounting: LoadAccounting;
}

export interface LoadsResponse {
  holiday: boolean;
  temperatureEntity: string;
  weatherEntity: string;
  devices: LoadDevice[];
  problems: string[];
  profile: LearnedLoadProfile | null;
  modelled: { entity: string; modelled: boolean }[];
}

/** The whole configuration, replaced as a unit by `PATCH /loads`. */
export interface LoadsInput {
  holiday: boolean;
  temperatureEntity: string;
  weatherEntity: string;
  devices: LoadDevice[];
}
