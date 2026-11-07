<script lang="ts">
  import { clock } from '../../lib/format/time';
  import type { DispatchResponse, PricesResponse } from '../../lib/api/types';

  interface Props {
    dispatch: DispatchResponse | null;
    prices: PricesResponse | null;
  }

  let { dispatch, prices }: Props = $props();

  const rows = $derived([
    {
      label: 'Dispatch log window',
      value:
        dispatch === null
          ? 'not loaded'
          : `${Math.round(dispatch.windowSeconds / 3600)} h · ${dispatch.count} entries`,
      note: 'from `GET /dispatch`',
    },
    {
      label: 'Price window',
      value: prices === null ? 'not loaded' : `${prices.count} published slots`,
      note: 'a request may span at most 8 days; gaps are never filled',
    },
    {
      label: 'Plan horizon',
      value: '120 slots · 30 h',
      note: 'one quarter-hour per slot',
    },
    {
      label: 'Savings rollups',
      value: 'day · month · year · lifetime',
      note: 'measured from settled slots, one row per local day',
    },
    {
      label: 'Money resolution',
      value: '1e-6 EUR',
      note: 'JSON rounded to 6 decimals; the ledger never rounds earlier',
    },
  ]);

  const window = $derived(
    prices === null ? null : `${clock(prices.from)} → ${clock(prices.to)} local`,
  );
</script>

<div class="space-y-2">
  <ul class="divide-y divide-line/70 rounded-lg border border-line">
    {#each rows as row (row.label)}
      <li class="px-2.5 py-1.5">
        <div class="flex items-baseline justify-between gap-2">
          <span class="text-[11px] text-muted">{row.label}</span>
          <span class="num shrink-0 text-[11px] text-ink">{row.value}</span>
        </div>
        <p class="text-[9.5px] text-faint">{row.note}</p>
      </li>
    {/each}
  </ul>
  <p class="text-[9.5px] text-faint">
    {window === null ? 'Price coverage unknown.' : `Currently published: ${window}.`}
    Retention limits are the contract's, not a client setting.
  </p>
</div>
