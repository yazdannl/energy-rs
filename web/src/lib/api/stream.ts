/**
 * `/api/v1/stream`, the engine's live view.
 *
 * `EventSource` is used rather than a hand-rolled fetch reader: it is the browser's own SSE
 * parser, it survives the ingress proxy and it costs one reconnect policy. The server coalesces
 * ticks, so a slow tab simply misses intermediate snapshots; that is the documented contract.
 */

import { apiRoot } from './client';
import type { LinkStatus, LiveFault, PlanEvent, StreamState } from './types';

export interface StreamHandlers {
  onState(state: StreamState): void;
  onPlan(event: PlanEvent): void;
  onFault(fault: LiveFault): void;
  onStatus(status: LinkStatus, detail?: string): void;
}

export interface StreamSource {
  start(handlers: StreamHandlers): void;
  stop(): void;
}

const MAX_BACKOFF_MS = 15_000;

export class HttpStream implements StreamSource {
  private source: EventSource | null = null;
  private handlers: StreamHandlers | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private delay = 1_000;

  start(handlers: StreamHandlers): void {
    this.handlers = handlers;
    this.open();
  }

  stop(): void {
    this.handlers = null;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.source?.close();
    this.source = null;
  }

  private open(): void {
    const handlers = this.handlers;
    if (handlers === null) return;
    handlers.onStatus('connecting');

    const source = new EventSource(new URL('stream', apiRoot()).toString());
    this.source = source;

    source.onopen = () => {
      this.delay = 1_000;
      handlers.onStatus('live');
    };

    source.addEventListener('state', (event) => {
      const state = parse<StreamState>(event);
      if (state !== null) handlers.onState(state);
    });
    source.addEventListener('plan', (event) => {
      const plan = parse<PlanEvent>(event);
      if (plan !== null) handlers.onPlan(plan);
    });
    source.addEventListener('fault', (event) => {
      const fault = parse<LiveFault>(event);
      if (fault !== null) handlers.onFault(fault);
    });

    source.onerror = () => {
      source.close();
      this.source = null;
      if (this.handlers === null) return;
      handlers.onStatus('reconnecting', `retrying in ${Math.round(this.delay / 1000)} s`);
      this.timer = setTimeout(() => {
        this.timer = null;
        this.open();
      }, this.delay);
      this.delay = Math.min(this.delay * 2, MAX_BACKOFF_MS);
    };
  }
}

function parse<T>(event: MessageEvent<string>): T | null {
  try {
    const value: unknown = JSON.parse(event.data);
    if (value === null || typeof value !== 'object') return null;
    return value as T;
  } catch {
    return null;
  }
}
