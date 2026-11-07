<script lang="ts">
  import BatteryMedium from '@lucide/svelte/icons/battery-medium';
  import PlugZap from '@lucide/svelte/icons/plug-zap';
  import Sun from '@lucide/svelte/icons/sun';

  import type { SavingsTotals } from '../../lib/api/types';
  import { fmtWh } from '../../lib/format/energy';
  import Tooltip from '../ui/Tooltip.svelte';

  interface Props {
    totals: SavingsTotals;
  }

  let { totals }: Props = $props();

  const selfConsumedWh = $derived(Math.max(0, totals.pvWh - totals.exportWh));
  const rows = $derived([
    {
      key: 'arbitrage',
      label: 'Battery arbitrage',
      value: fmtWh(totals.batteryDischargeWh),
      caption: `charged ${fmtWh(totals.batteryChargeWh)} · throughput, not a € split`,
    },
    {
      key: 'pv',
      label: 'PV self-consumption',
      value: fmtWh(selfConsumedWh),
      caption: `of ${fmtWh(totals.pvWh)} produced`,
    },
    {
      key: 'feedin',
      label: 'Feed-in',
      value: fmtWh(totals.exportWh),
      caption: 'exported energy',
    },
  ]);
</script>

<div class="space-y-2">
  <ul class="space-y-1.5">
    {#each rows as row (row.key)}
      <li
        class="flex items-center justify-between gap-3 rounded-lg border border-line/70 px-2.5 py-2"
      >
        <span class="flex min-w-0 items-center gap-2">
          {#if row.key === 'arbitrage'}
            <BatteryMedium class="size-3.5 shrink-0 text-batt" aria-hidden="true" />
          {:else if row.key === 'pv'}
            <Sun class="size-3.5 shrink-0 text-pv" aria-hidden="true" />
          {:else}
            <PlugZap class="size-3.5 shrink-0 text-grid" aria-hidden="true" />
          {/if}
          <span class="min-w-0">
            <span class="block text-[11.5px] text-ink">{row.label}</span>
            <span class="block text-[10px] text-faint">{row.caption}</span>
          </span>
        </span>
        <span class="num shrink-0 text-[12px] font-medium text-ink">{row.value}</span>
      </li>
    {/each}
  </ul>

  <p class="flex items-start gap-1.5 text-[10px] leading-relaxed text-faint">
    <Tooltip label="What the savings DTO can and cannot split">
      <span class="font-mono">savings</span> reports measured totals per day: actual, baseline and
      saving, plus import/export/PV and battery throughput. A per-category <em>euro</em> split (arbitrage
      vs. PV vs. feed-in) is not in the v1 contract, so none is shown here.
    </Tooltip>
    Energy per category is measured; the euro split per category is not exposed by the v1 savings contract.
  </p>
</div>
