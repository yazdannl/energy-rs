/**
 * The settings form: a draft on top of `GET /settings`, written with one `PATCH`.
 *
 * Only whitelisted keys are ever sent; the control mode has its own endpoint (`PATCH /mode`) and
 * a token is not part of the contract at all, so it cannot be read or written here.
 */

import { ApiError } from '../api/errors';
import type { SettingSchemaEntry, SettingValue, SettingsResponse } from '../api/types';

export class SettingsDraft {
  values = $state<Record<string, SettingValue>>({});
  schema = $state<SettingSchemaEntry[]>([]);
  asOf = $state<string | null>(null);
  loading = $state(true);
  saving = $state(false);
  error = $state<ApiError | null>(null);
  private saved: Record<string, SettingValue> = {};

  apply(response: SettingsResponse): void {
    this.values = { ...response.values };
    this.saved = { ...response.values };
    this.schema = response.schema;
    this.asOf = response.asOf;
    this.error = null;
    this.loading = false;
  }

  async load(fetchSettings: () => Promise<SettingsResponse>): Promise<void> {
    this.loading = true;
    try {
      this.apply(await fetchSettings());
    } catch (cause) {
      this.error = cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
      this.loading = false;
    }
  }

  set(key: string, value: SettingValue): void {
    this.values = { ...this.values, [key]: value };
  }

  get dirty(): string[] {
    return Object.keys(this.values).filter((key) => this.values[key] !== this.saved[key]);
  }

  reset(): void {
    this.values = { ...this.saved };
  }

  entry(key: string): SettingSchemaEntry | undefined {
    return this.schema.find((item) => item.key === key);
  }

  value(key: string): SettingValue {
    return this.values[key] ?? null;
  }

  number(key: string, fallback = 0): number {
    const value = this.values[key];
    return typeof value === 'number' ? value : fallback;
  }

  text(key: string, fallback = ''): string {
    const value = this.values[key];
    return typeof value === 'string' ? value : fallback;
  }

  async save(
    write: (values: Record<string, SettingValue>) => Promise<SettingsResponse>,
  ): Promise<ApiError | null> {
    const keys = this.dirty;
    if (keys.length === 0) return null;
    this.saving = true;
    try {
      const patch: Record<string, SettingValue> = {};
      for (const key of keys) patch[key] = this.values[key];
      this.apply(await write(patch));
      return null;
    } catch (cause) {
      const error = cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
      this.error = error;
      return error;
    } finally {
      this.saving = false;
    }
  }
}
