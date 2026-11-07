<script lang="ts">
  import BatteryMedium from '@lucide/svelte/icons/battery-medium';
  import Check from '@lucide/svelte/icons/check';
  import PlugZap from '@lucide/svelte/icons/plug-zap';
  import Sun from '@lucide/svelte/icons/sun';

  import type { SavingsTotals } from '../../lib/api/types';
  import { fmtWh } from '../../lib/format/energy';
  import { fmtEur, fmtEur6, round6 } from '../../lib/format/money';
  import { moneyTone } from '../../lib/format/sign';
  import { BADGE, TEXT } from '../../lib/uiTone';
  import Tooltip from '../ui/Tooltip.svelte';

  interface Props {
    totals: SavingsTotals;
  }

  let { totals }: Props = $props();

  const components = $derived([
    {
      key: 'arbitrage',
      label: 'Battery arbitrage',
      value: totals.arbitrageEur,
      hint: 'Cheap import stored, dear import avoided.',
    },
    {
      key: 'self',
      label: 'PV self-consumption',
      value: totals.selfConsumptionEur,
      hint: 'Roof energy used in the house instead of being imported.',
    },
    {
      key: 'feedin',
      label: 'Feed-in',
      value: totals.feedInEur,
      hint: 'Export revenue against the counterfactual; it can be negative.',
    },
    {
      key: 'balance',
      label: 'Battery balance',
      value: totals.batteryBalanceEur,
      hint: 'The pack’s own energy balance: round-trip losses and saldering effects.',
    },
  ]);

  const sum = $derived(round6(components.reduce((total, item) => total + item.value, 0)));
  const maxAbs = $derived(Math.max(0.000001, ...components.map((item) => Math.abs(item.value))));
  const exact = $derived(Math.abs(sum - totals.savingEur) < 1e-6);

  const selfConsumedWh = $derived(Math.max(0, totals.pvWh - totals.exportWh));
</script>

<div class="space-y-2">
  <ul class="space-y-1.5">
    {#each components as item (item.key)}
      <li class="rounded-lg border border-line/70 px-2.5 py-2">
        <div class="flex items-baseline justify-between gap-2">
          <span class="flex min-w-0 items-center gap-1.5">
            {#if item.key === 'arbitrage'}
              <BatteryMedium class="size-3.5 shrink-0 text-batt" aria-hidden="true" />
            {:else if item.key === 'self'}
              <Sun class="size-3.5 shrink-0 text-pv" aria-hidden="true" />
            {:else if item.key === 'feedin'}
              <PlugZap class="size-3.5 shrink-0 text-grid" aria-hidden="true" />
            {:else}
              <span
                class="size-3.5 shrink-0 rounded-full border border-line-strong"
                aria-hidden="true"
              ></span>
            {/if}
            <span class="truncate text-[11.5px] text-ink">{item.label}</span>
          </span>
          <span
            class={['num shrink-0 text-[12px] font-medium', TEXT[moneyTone(item.value)]]}
            title={fmtEur6(item.value)}
          >
            {fmtEur(item.value)}
          </span>
        </div>
        <div class="mt-1 flex items-center gap-2">
          <div class="h-[3px] flex-1 overflow-hidden rounded-full bg-line">
            <div
              class={['h-full rounded-full', item.value >= 0 ? 'bg-good' : 'bg-bad']}
              style="width:{Math.max(1, (Math.abs(item.value) / maxAbs) * 100)}%"
            ></div>
          </div>
        </div>
        <p class="mt-1 text-[9.5px] leading-relaxed text-faint">{item.hint}</p>
      </li>
    {/each}
  </ul>

  <p
    class="flex items-center justify-between gap-2 border-t border-line pt-2 text-[10.5px] text-muted"
  >
    <span class="flex items-center gap-1">
      {#if exact}
        <Check class="size-3 text-good" aria-hidden="true" />
      {/if}
      components sum to the saving
    </span>
    <span class="num text-ink">{fmtEur(sum)}</span>
  </p>
  {#if !exact}
    <p class={['rounded-md border px-2 py-1 text-[10px]', BADGE.warn]}>
      The four components do not add up to the reported saving ({fmtEur(totals.savingEur)}) — the
      ledger and the split disagree, and the split is shown as it came.
    </p>
  {/if}

  <div class="flex items-start gap-1.5 text-[10px] leading-relaxed text-faint">
    <Tooltip label="What the split fields mean">
      <span class="font-mono">arbitrageEur</span>,
      <span class="font-mono">selfConsumptionEur</span>,
      <span class="font-mono">feedInEur</span> and <span class="font-mono">batteryBalanceEur</span>
      are measured per day and sum to <span class="font-mono">savingEur</span> exactly (`docs/SAVINGS.md`).
      A day with `null` money is excluded from these totals.
    </Tooltip>
    Measured: charged {fmtWh(totals.batteryChargeWh)} · discharged
    {fmtWh(totals.batteryDischargeWh)} · PV {fmtWh(totals.pvWh)} (self-consumed {fmtWh(
      selfConsumedWh,
    )})
  </div>
</div>
