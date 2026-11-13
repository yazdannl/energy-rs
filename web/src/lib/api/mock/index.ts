/**
 * The `?mock=1` backend: the same `ApiPort` and `StreamSource` interfaces as HTTP, so every
 * screen runs without the add-on. It is a development aid, never a fallback: the UI never
 * silently switches to it.
 */

import type { ApiPort } from '../client';
import type { StreamHandlers, StreamSource } from '../stream';
import type {
  DispatchResponse,
  Health,
  LearningResponse,
  LoadsInput,
  LoadsResponse,
  ReplanResponse,
  Mode,
  ModeResponse,
  OverrideAction,
  OverrideResponse,
  PackAlias,
  PacksResponse,
  Plan,
  PlansResponse,
  PricesResponse,
  ProtectionResponse,
  Savings,
  SavingsPeriod,
  SettingValue,
  SettingsResponse,
  Site,
} from '../types';
import {
  MOCK_MODE,
  makeDispatch,
  makeHealth,
  makeLoads,
  makePacks,
  makePlan,
  makePrices,
  makeSavings,
  makeSettings,
  makeSite,
  makeStreamState,
} from './fixtures';

const LATENCY_MS = 160;

async function latency(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, LATENCY_MS));
}

/** Mode is shared between the command endpoints and the stream so the mock stays coherent. */
const shared: { mode: Mode; holiday: boolean } = { mode: MOCK_MODE, holiday: false };

/** Aliases the mock remembers, keyed by serial; `null` is a cleared label. */
const aliases = new Map<string, string | null>();

function rememberedAlias<T extends { sn: string; alias: string | null }>(pack: T): T {
  if (!aliases.has(pack.sn)) return pack;
  return { ...pack, alias: aliases.get(pack.sn) ?? null };
}

export class MockApi implements ApiPort {
  readonly kind = 'mock' as const;

  async health(): Promise<Health> {
    await latency();
    return makeHealth(new Date());
  }

  async site(): Promise<Site> {
    await latency();
    const site = makeSite(new Date());
    return { ...site, mode: shared.mode, packs: site.packs.map(rememberedAlias) };
  }

  async packs(): Promise<PacksResponse> {
    await latency();
    const response = makePacks(new Date());
    return { packs: response.packs.map(rememberedAlias) };
  }

  async setPackAlias(sn: string, alias: string | null): Promise<PackAlias> {
    await latency();
    const trimmed = alias?.trim() ?? '';
    const stored = trimmed === '' ? null : trimmed.slice(0, 40);
    aliases.set(sn, stored);
    return { sn, alias: stored };
  }

  async prices(from?: string, to?: string): Promise<PricesResponse> {
    await latency();
    const start = from === undefined ? new Date() : new Date(from);
    const count =
      to === undefined
        ? 192
        : Math.max(1, Math.round((new Date(to).getTime() - start.getTime()) / 900_000));
    return makePrices(start, Math.min(count, 192));
  }

  async currentPlan(): Promise<Plan | null> {
    await latency();
    return { ...makePlan(new Date()), mode: shared.mode };
  }

  async plans(limit = 12): Promise<PlansResponse> {
    await latency();
    const plan = makePlan(new Date());
    return {
      count: Math.min(limit, 3),
      plans: Array.from({ length: Math.min(limit, 3) }, (_, index) => ({
        planId: plan.planId - index,
        createdAt: plan.createdAt,
        mode: shared.mode,
        horizonStartAt: plan.horizonStartAt,
        horizonEndAt: plan.horizonEndAt,
        costEur: plan.summary.costEur + index * 0.031,
        baselineCostEur: plan.summary.baselineCostEur,
        savedEur: plan.summary.savedEur - index * 0.031,
        feasible: true,
        infeasibleReason: null,
      })),
    };
  }

  async dispatch(limit = 25): Promise<DispatchResponse> {
    await latency();
    const response = makeDispatch(new Date());
    return { ...response, entries: response.entries.slice(0, limit) };
  }

  async savings(period: SavingsPeriod): Promise<Savings> {
    await latency();
    return makeSavings(new Date(), period);
  }

  async settings(): Promise<SettingsResponse> {
    await latency();
    return makeSettings(new Date());
  }

  async saveSettings(values: Record<string, SettingValue>): Promise<SettingsResponse> {
    await latency();
    const current = makeSettings(new Date());
    return { ...current, values: { ...current.values, ...values } };
  }

  async loads(): Promise<LoadsResponse> {
    await latency();
    return makeLoads(new Date(), shared.holiday);
  }

  async saveLoads(input: LoadsInput): Promise<LoadsResponse> {
    await latency();
    shared.holiday = input.holiday;
    const current = makeLoads(new Date(), input.holiday);
    return {
      ...current,
      temperatureEntity: input.temperatureEntity,
      weatherEntity: input.weatherEntity,
      loadEntity: input.loadEntity,
      devices: input.devices,
      modelled: input.devices.map((device) => ({ entity: device.entity, modelled: true })),
    };
  }

  async setMode(mode: Mode): Promise<ModeResponse> {
    await latency();
    shared.mode = mode;
    return {
      mode,
      updatedAt: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
      engineNotified: true,
    };
  }

  async override(input: {
    pack: string;
    action: OverrideAction;
    powerW?: number;
    ttlS?: number;
  }): Promise<OverrideResponse> {
    await latency();
    const ttlS = input.ttlS ?? 900;
    return {
      accepted: true,
      pack: input.pack,
      action: input.action,
      powerW: input.action === 'charge' ? (input.powerW ?? 800) : null,
      ttlS,
      expiresAt: new Date(Date.now() + ttlS * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z'),
    };
  }

  async fullCharge(pack?: string): Promise<ProtectionResponse> {
    await latency();
    return { accepted: true, action: 'full_charge', pack: pack ?? null };
  }

  async replan(): Promise<ReplanResponse> {
    return { accepted: true };
  }

  async relearn(pack?: string): Promise<LearningResponse> {
    await latency();
    return { accepted: true, action: 'relearn', pack: pack ?? null };
  }
}

export class MockStream implements StreamSource {
  private handlers: StreamHandlers | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private tick = 0;

  start(handlers: StreamHandlers): void {
    this.handlers = handlers;
    handlers.onStatus('connecting');
    setTimeout(() => {
      if (this.handlers === null) return;
      handlers.onStatus('live');
      this.emit();
      this.timer = setInterval(() => this.emit(), 4_000);
    }, 250);
  }

  stop(): void {
    this.handlers = null;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }

  private emit(): void {
    const handlers = this.handlers;
    if (handlers === null) return;
    this.tick += 1;
    handlers.onState({ ...makeStreamState(new Date(), this.tick), mode: shared.mode });
    if (this.tick % 20 === 0) handlers.onPlan({ planId: 41, at: new Date().toISOString() });
  }
}
