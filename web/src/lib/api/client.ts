/**
 * The typed HTTP client for `/api/v1`.
 *
 * Every URL is built from `document.baseURI`, so the same bundle works at the add-on root and
 * behind the Home Assistant ingress path (`/api/hassio_ingress/<token>/`): the API prefix is
 * `./api/v1/`, never `/api/v1/`.
 */

import { ApiError } from './errors';
import type {
  DispatchResponse,
  Health,
  LearningResponse,
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
  ReplanResponse,
  Savings,
  SavingsPeriod,
  SettingValue,
  SettingsResponse,
  Site,
} from './types';

/** Everything the screens need from the backend, so mock and HTTP are interchangeable. */
export interface ApiPort {
  readonly kind: 'http' | 'mock';
  health(): Promise<Health>;
  site(): Promise<Site>;
  packs(): Promise<PacksResponse>;
  /** Set or clear the operator's alias; the answer is the stored `{sn, alias}`. */
  setPackAlias(sn: string, alias: string | null): Promise<PackAlias>;
  prices(from?: string, to?: string): Promise<PricesResponse>;
  /** `null` when the API answers `404 not_found`: no plan has been solved yet. */
  currentPlan(): Promise<Plan | null>;
  plans(limit?: number): Promise<PlansResponse>;
  dispatch(limit?: number): Promise<DispatchResponse>;
  savings(period: SavingsPeriod): Promise<Savings>;
  settings(): Promise<SettingsResponse>;
  saveSettings(values: Record<string, SettingValue>): Promise<SettingsResponse>;
  setMode(mode: Mode): Promise<ModeResponse>;
  override(input: {
    pack: string;
    action: OverrideAction;
    powerW?: number;
    ttlS?: number;
  }): Promise<OverrideResponse>;
  fullCharge(pack?: string): Promise<ProtectionResponse>;
  relearn(pack?: string): Promise<LearningResponse>;
  /** Ask the engine for a fresh solve; the plan arrives with the next snapshot. */
  replan(): Promise<ReplanResponse>;
}

export function apiRoot(baseURI?: string): URL {
  const base =
    baseURI ?? (typeof document === 'undefined' ? 'http://localhost/' : document.baseURI);
  return new URL('api/v1/', collapseSlashes(base));
}

/**
 * Collapse repeated slashes in a URL's path, keeping the scheme's `//`.
 *
 * Home Assistant reports an add-on's `ingress_url` as `<entry>//`, so the panel's `document.baseURI`
 * can end in two slashes. Resolved naively that yields `//api/v1/...`, which the router answers with
 * 404 -- while `//` itself still serves the SPA shell, so the UI looks connected but stays empty.
 */
export function collapseSlashes(url: string): string {
  return url.replace(/([^:/])\/{2,}/g, '$1/');
}

function query(params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) search.set(key, String(value));
  }
  const encoded = search.toString();
  return encoded === '' ? '' : `?${encoded}`;
}

export class HttpApi implements ApiPort {
  readonly kind = 'http' as const;
  private readonly root: URL;

  constructor(root: URL = apiRoot()) {
    this.root = root;
  }

  private url(path: string, params: Record<string, string | number | undefined> = {}): string {
    return new URL(`${path}${query(params)}`, this.root).toString();
  }

  private async get<T>(
    path: string,
    params: Record<string, string | number | undefined> = {},
  ): Promise<T> {
    const response = await fetch(this.url(path, params), {
      headers: { accept: 'application/json' },
    });
    if (!response.ok) throw await ApiError.fromResponse(response);
    return (await response.json()) as T;
  }

  private async command<T>(path: string, method: 'PATCH' | 'POST', body: unknown): Promise<T> {
    const response = await fetch(this.url(path), {
      method,
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!response.ok) throw await ApiError.fromResponse(response);
    return (await response.json()) as T;
  }

  health(): Promise<Health> {
    return this.get<Health>('health');
  }

  site(): Promise<Site> {
    return this.get<Site>('site');
  }

  packs(): Promise<PacksResponse> {
    return this.get<PacksResponse>('packs');
  }

  setPackAlias(sn: string, alias: string | null): Promise<PackAlias> {
    return this.command<PackAlias>(`packs/${encodeURIComponent(sn)}`, 'PATCH', { alias });
  }

  prices(from?: string, to?: string): Promise<PricesResponse> {
    return this.get<PricesResponse>('prices', { from, to });
  }

  async currentPlan(): Promise<Plan | null> {
    const response = await fetch(this.url('plan/current'), {
      headers: { accept: 'application/json' },
    });
    if (response.status === 404) return null;
    if (!response.ok) throw await ApiError.fromResponse(response);
    return (await response.json()) as Plan;
  }

  plans(limit = 12): Promise<PlansResponse> {
    return this.get<PlansResponse>('plans', { limit });
  }

  dispatch(limit = 25): Promise<DispatchResponse> {
    return this.get<DispatchResponse>('dispatch', { limit });
  }

  savings(period: SavingsPeriod): Promise<Savings> {
    return this.get<Savings>('savings', { period });
  }

  settings(): Promise<SettingsResponse> {
    return this.get<SettingsResponse>('settings');
  }

  saveSettings(values: Record<string, SettingValue>): Promise<SettingsResponse> {
    return this.command<SettingsResponse>('settings', 'PATCH', { values });
  }

  setMode(mode: Mode): Promise<ModeResponse> {
    return this.command<ModeResponse>('mode', 'PATCH', { mode });
  }

  override(input: {
    pack: string;
    action: OverrideAction;
    powerW?: number;
    ttlS?: number;
  }): Promise<OverrideResponse> {
    return this.command<OverrideResponse>('override', 'POST', input);
  }

  fullCharge(pack?: string): Promise<ProtectionResponse> {
    return this.command<ProtectionResponse>('protection', 'POST', { action: 'full_charge', pack });
  }

  replan(): Promise<ReplanResponse> {
    return this.command<ReplanResponse>('replan', 'POST', {});
  }

  relearn(pack?: string): Promise<LearningResponse> {
    return this.command<LearningResponse>('learning', 'POST', { action: 'relearn', pack });
  }
}
