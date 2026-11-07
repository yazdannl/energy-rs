/**
 * A one-shot resource with honest states: loading, data, error.
 *
 * Deliberately not a cache: every screen owns its fetch, so a stale number is never shown as a
 * fresh one (the API answers reads from the ledger, and the ledger is the truth).
 */

import { ApiError } from '../api/errors';

export class Resource<T> {
  data = $state<T | null>(null);
  error = $state<ApiError | null>(null);
  loading = $state(true);
  private readonly loader: () => Promise<T>;
  private token = 0;

  constructor(loader: () => Promise<T>) {
    this.loader = loader;
  }

  async load(): Promise<void> {
    const token = ++this.token;
    this.loading = true;
    try {
      const value = await this.loader();
      if (token !== this.token) return;
      this.data = value;
      this.error = null;
    } catch (cause) {
      if (token !== this.token) return;
      this.error = cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
    } finally {
      if (token === this.token) this.loading = false;
    }
  }

  /** Drop the in-flight answer and forget the data; used on unmount. */
  dispose(): void {
    this.token += 1;
  }
}

export function resource<T>(loader: () => Promise<T>): Resource<T> {
  return new Resource(loader);
}
