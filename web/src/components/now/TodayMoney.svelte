<script lang="ts">
  import TrendingUp from '@lucide/svelte/icons/trending-up';

  import type { Savings } from '../../lib/api/types';
  import { fmtEur, fmtEur6 } from '../../lib/format/money';
  import { fmtWh } from '../../lib/format/energy';
  import { moneyTone } from '../../lib/format/sign';
  import { TEXT } from '../../lib/uiTone';
  import Badge from '../ui/Badge.svelte';
  import EmptyNote from '../ui/EmptyNote.svelte';

  interface Props {
    savings: Savings | null;
  }

  let { savings }: Props = $props();

  const totals = $derived(savings?.totals ?? null);
  const hasData = $derived((totals?.days ?? 0) > 0);
  const today = $derived(savings?.days.at(-1) ?? null);
  const tone = $derived(moneyTone(today?.savingEur));
</script>

{#if !hasData || totals === null}
  <EmptyNote
    title="No savings rows yet"
    detail="Savings are measured from settled slots, never from the plan. The first complete day appears after a full local day is settled."
  />
{:else}
  <div class="space-y-3">
    <div class="flex items-end justify-between gap-2">
      <div>
        <p class="text-[10px] uppercase tracking-[0.07em] text-faint">Saved today</p>
        <p class={['num mt-0.5 text-[26px] leading-none font-semibold', TEXT[tone]]}>
          {fmtEur(today?.savingEur ?? 0)}
        </p>
        <p class="mt-1 text-[10.5px] text-faint" title={fmtEur6(today?.savingEur)}>
          vs. {fmtEur(today?.baselineEur ?? 0)} without the battery
        </p>
      </div>
      <Badge tone={today?.complete === true ? 'good' : 'warn'}>
        {today?.complete === true
          ? 'complete day'
          : `${(today?.coverageHours ?? 0).toFixed(1)} h covered`}
      </Badge>
    </div>

    <div class="grid grid-cols-3 gap-2 border-t border-line pt-2.5">
      <div>
        <p class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Actual</p>
        <p class="num text-[12px] text-ink">{fmtEur(today?.actualEur)}</p>
      </div>
      <div>
        <p class="text-[9.5px] uppercase tracking-[0.07em] text-faint">Import</p>
        <p class="num text-[12px] text-ink">{fmtWh(today?.importWh)}</p>
      </div>
      <div>
        <p class="text-[9.5px] uppercase tracking-[0.07em] text-faint">PV</p>
        <p class="num text-[12px] text-pv">{fmtWh(today?.pvWh)}</p>
      </div>
    </div>

    <p class="flex items-center gap-1.5 text-[10px] text-faint">
      <TrendingUp class="size-3 text-good" aria-hidden="true" />
      {fmtEur(totals.savingEur)} saved over {totals.days} day{totals.days === 1 ? '' : 's'}
    </p>
  </div>
{/if}
