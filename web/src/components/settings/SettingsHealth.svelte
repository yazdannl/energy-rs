<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';

  import type { Health } from '../../lib/api/types';
  import { clockSeconds } from '../../lib/format/time';
  import { connection } from '../../lib/state/connection.svelte';
  import { mockMode } from '../../lib/state/runtime';
  import Badge from '../ui/Badge.svelte';
  import ErrorNote from '../ui/ErrorNote.svelte';
  import type { ApiError } from '../../lib/api/errors';

  interface Props {
    health: Health | null;
    error: ApiError | null;
    onretry: () => void;
  }

  let { health, error, onretry }: Props = $props();

  const ok = $derived(health?.status === 'ok');
</script>

{#if error}
  <ErrorNote {error} {onretry} />
{:else}
  <div class="space-y-2">
    <div class="flex flex-wrap items-center gap-1.5">
      <Badge tone={ok ? 'good' : 'bad'}>{health?.status ?? 'unknown'}</Badge>
      <Badge tone="neutral">v{health?.version ?? '—'}</Badge>
      <Badge tone={connection.status === 'live' ? 'good' : 'warn'}>SSE {connection.status}</Badge>
      {#if mockMode}<Badge tone="grid">fixtures</Badge>{/if}
    </div>
    <dl class="grid grid-cols-2 gap-x-3 gap-y-1.5 text-[11px]">
      <div>
        <dt class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Mode</dt>
        <dd class="text-ink">{health?.mode ?? '—'}</dd>
      </div>
      <div>
        <dt class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Last tick</dt>
        <dd class="num text-ink">{clockSeconds(health?.lastTickAt)}</dd>
      </div>
      <div>
        <dt class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Ledger</dt>
        <dd class="text-ink">{health ? 'answering' : 'no answer'}</dd>
      </div>
      <div>
        <dt class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Engine</dt>
        <dd class="text-ink">{health?.lastTickAt ? 'publishing' : 'no snapshot yet'}</dd>
      </div>
    </dl>
    <p class="flex items-start gap-1.5 text-[9.5px] leading-relaxed text-faint">
      <Activity class="mt-[1px] size-3 shrink-0" aria-hidden="true" />
      `GET /health` touches no database and no Home Assistant: it is the Supervisor watchdog contract,
      so a green badge does not by itself mean the engine is solving plans. `lastTickAt` is the honest
      answer to that.
    </p>
  </div>
{/if}
