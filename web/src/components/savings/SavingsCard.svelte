<script lang="ts">
  import { fmtEur, fmtEur6 } from '../../lib/format/money';
  import { moneyTone } from '../../lib/format/sign';
  import { TEXT, type Tone } from '../../lib/uiTone';
  import Badge from '../ui/Badge.svelte';

  interface Props {
    label: string;
    saving: number | null;
    baseline: number | null;
    actual: number | null;
    days: number;
    completeDays: number;
    /** Days that actually contributed to the money total (`null` days are excluded by the API). */
    countedDays: number;
    note?: string;
  }

  let { label, saving, baseline, actual, days, completeDays, countedDays, note }: Props = $props();

  const tone: Tone = $derived(moneyTone(saving));
  const hasData = $derived(countedDays > 0 && saving !== null);
</script>

<div class="card px-3.5 py-3">
  <div class="flex items-center justify-between gap-2">
    <p class="text-[10.5px] uppercase tracking-[0.07em] text-faint">{label}</p>
    {#if hasData}
      <Badge tone={completeDays === days ? 'good' : 'warn'}>
        {days === 1
          ? completeDays === 1
            ? 'complete'
            : 'partial'
          : `${completeDays}/${days} days`}
      </Badge>
    {/if}
  </div>

  {#if !hasData}
    <p class="num mt-2 text-[22px] leading-none font-semibold text-faint">—</p>
    <p class="mt-1 text-[10.5px] text-faint">no data</p>
  {:else}
    <p
      class={['num mt-2 text-[22px] leading-none font-semibold', TEXT[tone]]}
      title={fmtEur6(saving)}
    >
      {fmtEur(saving)}
    </p>
    <p class="mt-1 text-[10.5px] text-muted">
      {fmtEur(actual)} spent vs {fmtEur(baseline)} baseline
    </p>
  {/if}
  {#if note}
    <p class="mt-1 text-[10px] text-faint">{note}</p>
  {/if}
</div>
