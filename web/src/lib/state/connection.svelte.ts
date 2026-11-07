/**
 * The single live connection to the engine's SSE stream.
 *
 * The screens read `snapshot` for now-values and `planVersion` to know that a new plan was solved
 * (the `plan` event carries only an id, so the page re-reads `/plan/current`).
 */

import type { LinkStatus, LiveFault, Mode, StreamState } from '../api/types';
import type { StreamHandlers, StreamSource } from '../api/stream';

class Connection {
  status = $state<LinkStatus>('connecting');
  detail = $state<string | null>(null);
  snapshot = $state<StreamState | null>(null);
  lastEventAt = $state<string | null>(null);
  planVersion = $state(0);
  faults = $state<LiveFault[]>([]);
  private source: StreamSource | null = null;
  private started = false;

  start(source: StreamSource): void {
    if (this.started) return;
    this.started = true;
    this.source = source;
    const handlers: StreamHandlers = {
      onStatus: (status, detail) => {
        this.status = status;
        this.detail = detail ?? null;
      },
      onState: (state) => {
        this.snapshot = state;
        this.lastEventAt = new Date().toISOString();
        this.faults = state.faults;
      },
      onPlan: () => {
        this.planVersion += 1;
      },
      onFault: (fault) => {
        this.faults = [...this.faults.filter((item) => item.kind !== fault.kind), fault];
      },
    };
    source.start(handlers);
  }

  stop(): void {
    this.source?.stop();
    this.source = null;
    this.started = false;
    this.status = 'offline';
  }

  /** Local echo of a mode write; the next `state` event confirms it. */
  noteMode(mode: Mode): void {
    if (this.snapshot !== null) this.snapshot = { ...this.snapshot, mode };
  }
}

export const connection = new Connection();
