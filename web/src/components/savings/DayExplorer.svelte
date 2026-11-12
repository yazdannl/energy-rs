<script lang="ts">
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  import type { SavingsDay } from '../../lib/api/types';
  import { fmtEur, fmtEurPerKwh, round6 } from '../../lib/format/money';
  import { dayLong } from '../../lib/format/time';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import EmptyNote from '../ui/EmptyNote.svelte';

  interface Props {
    days: SavingsDay[];
  }

  let { days }: Props = $props();

  // Newest first, so "previous" walks back in time the way a calendar would.
  const ordered = $derived([...days].sort((a, b) => (a.day < b.day ? 1 : -1)));
  let index = $state(0);

  const selected = $derived(ordered[Math.min(index, Math.max(0, ordered.length - 1))] ?? null);
  const canBack = $derived(index < ordered.length - 1);
  const canForward = $derived(index > 0);

  const split = $derived.by(() => {
    const day = selected;
    if (day === null) return null;
    const parts = [day.arbitrageEur, day.selfConsumptionEur, day.feedInEur, day.batteryBalanceEur];
    if (parts.some((part) => part === null) || day.savingEur === null) return null;
    const sum = round6(parts.reduce<number>((total, part) => total + (part ?? 0), 0));
    return { sum, reconciles: Math.abs(sum - day.savingEur) < 1e-6 };
  });

  /** Per-kWh averages, only where the energy actually flowed. */
  const averages = $derived.by(() => {
    const day = selected;
    if (day === null) return [];
    const rows: { label: string; value: string }[] = [];
    if (day.importWh > 0 && day.actualEur !== null)
      rows.push({
        label: 'paid per imported kWh',
        value: fmtEurPerKwh(day.actualEur / (day.importWh / 1000)),
      });
    if (day.pvWh > 0 && day.batteryChargeWh > 0)
      rows.push({
        label: 'PV share of the charge',
        value: `${Math.round(Math.min(100, (day.pvWh / day.batteryChargeWh) * 100))} %`,
      });
    if (day.batteryDischargeWh > 0)
      rows.push({
        label: 'delivered by the pack',
        value: `${(day.batteryDischargeWh / 1000).toFixed(2)} kWh`,
      });
    return rows;
  });

  const energy = $derived.by(() => {
    const day = selected;
    if (day === null) return [];
    return [
      { label: 'imported', wh: day.importWh },
      { label: 'exported', wh: day.exportWh },
      { label: 'PV', wh: day.pvWh },
      { label: 'charged into the pack', wh: day.batteryChargeWh },
      { label: 'discharged from the pack', wh: day.batteryDischargeWh },
    ];
  });

  const money = $derived.by(() => {
    const day = selected;
    if (day === null) return [];
    return [
      { label: 'actual', value: day.actualEur },
      { label: 'baseline (no battery)', value: day.baselineEur },
      { label: 'saving', value: day.savingEur },
      { label: 'arbitrage', value: day.arbitrageEur },
      { label: 'self-consumption', value: day.selfConsumptionEur },
      { label: 'feed-in', value: day.feedInEur },
      { label: 'balance', value: day.batteryBalanceEur },
    ];
  });
</script>

{#if selected === null}
  <EmptyNote title="No days yet" detail="The rollup writes a row once a day has measurements." />
{:else}
  <div class="flex items-center justify-between gap-2">
    <div class="flex items-center gap-1.5">
      <Button
        variant="ghost"
        disabled={!canBack}
        onclick={() => (index = Math.min(ordered.length - 1, index + 1))}
      >
        <ChevronLeft class="size-3.5" />
      </Button>
      <p class="num text-[12px] text-ink">{dayLong(selected.day)}</p>
      <Button
        variant="ghost"
        disabled={!canForward}
        onclick={() => (index = Math.max(0, index - 1))}
      >
        <ChevronRight class="size-3.5" />
      </Button>
      <span class="text-[10px] text-faint">{index + 1} / {ordered.length}</span>
    </div>
    <Badge tone={selected.complete ? 'good' : selected.savingEur === null ? 'warn' : 'neutral'}>
      {selected.complete ? 'complete' : selected.savingEur === null ? 'no money' : 'partial'}
    </Badge>
  </div>

  <div class="grid grid-cols-2 gap-x-3 gap-y-2 sm:grid-cols-4">
    {#each money as row (row.label)}
      <div>
        <p class="text-[9.5px] uppercase tracking-[0.06em] text-faint">{row.label}</p>
        <p class="num text-[12.5px] text-ink">{fmtEur(row.value)}</p>
      </div>
    {/each}
  </div>

  <div class="grid grid-cols-2 gap-x-3 gap-y-2 sm:grid-cols-5">
    {#each energy as row (row.label)}
      <div>
        <p class="text-[9.5px] uppercase tracking-[0.06em] text-faint">{row.label}</p>
        <p class="num text-[12px] text-muted">{(row.wh / 1000).toFixed(2)} kWh</p>
      </div>
    {/each}
  </div>

  {#if split !== null}
    <div
      class="rounded-md border border-line/60 bg-panel/40 p-2 text-[10.5px] leading-relaxed text-muted"
    >
      <p class="num">
        arbitrage {fmtEur(selected.arbitrageEur)} + self-consumption {fmtEur(
          selected.selfConsumptionEur,
        )} + feed-in {fmtEur(selected.feedInEur)} + balance {fmtEur(selected.batteryBalanceEur)}
        = {fmtEur(split.sum)}
        <span class={split.reconciles ? 'text-good' : 'text-warn'}>
          {split.reconciles ? '✓ matches the stored saving' : '≠ the stored saving'}
        </span>
      </p>
      <p class="num">
        baseline {fmtEur(selected.baselineEur)} − actual {fmtEur(selected.actualEur)} = saving
        {fmtEur(selected.savingEur)}
      </p>
    </div>
  {:else}
    <p class="text-[10.5px] leading-relaxed text-faint">
      No money for this day: {selected.coverageHours.toFixed(2)} h of measurements
      {selected.savingEur === null
        ? '(below the 80 % coverage a closed day needs, or a price gap)'
        : ''}.
    </p>
  {/if}

  {#if averages.length > 0}
    <div class="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-faint">
      {#each averages as row (row.label)}
        <span>{row.label}: <span class="num text-muted">{row.value}</span></span>
      {/each}
    </div>
  {/if}
{/if}
