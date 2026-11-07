<script lang="ts">
  import BatteryMedium from '@lucide/svelte/icons/battery-medium';
  import Coins from '@lucide/svelte/icons/coins';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import TrendingUp from '@lucide/svelte/icons/trending-up';

  import type { Plan } from '../../lib/api/types';
  import { fmtEur, fmtEur6 } from '../../lib/format/money';
  import { fmtWh } from '../../lib/format/energy';
  import { moneyTone } from '../../lib/format/sign';
  import Badge from '../ui/Badge.svelte';
  import StatTile from '../ui/StatTile.svelte';

  interface Props {
    plan: Plan;
  }

  let { plan }: Props = $props();

  const summary = $derived(plan.summary);
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center gap-1.5">
    <Badge tone={plan.feasible ? 'good' : 'bad'}>{plan.feasible ? 'feasible' : 'infeasible'}</Badge>
    <Badge tone="neutral">plan #{plan.planId}</Badge>
    {#each plan.bindingReasons as reason (reason)}
      <Badge tone="info">{reason}</Badge>
    {/each}
    {#if summary.infeasibleReason}
      <Badge tone="bad">{summary.infeasibleReason}</Badge>
    {/if}
  </div>

  <div class="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">
    <StatTile
      label="Cost"
      value={fmtEur(summary.costEur)}
      tone={moneyTone(-summary.costEur)}
      exact={fmtEur6(summary.costEur)}
    >
      {#snippet icon()}<Coins class="size-3" />{/snippet}
    </StatTile>
    <StatTile
      label="Baseline"
      value={fmtEur(summary.baselineCostEur)}
      exact={fmtEur6(summary.baselineCostEur)}
    >
      {#snippet icon()}<RefreshCw class="size-3" />{/snippet}
    </StatTile>
    <StatTile
      label="Saved"
      value={fmtEur(summary.savedEur)}
      tone={moneyTone(summary.savedEur)}
      exact={fmtEur6(summary.savedEur)}
    >
      {#snippet icon()}<TrendingUp class="size-3" />{/snippet}
    </StatTile>
    <StatTile label="Charged" value={fmtWh(summary.chargedWh)} />
    <StatTile label="Discharged" value={fmtWh(summary.dischargedWh)} />
    <StatTile
      label="Cycles"
      value={summary.cycles.toFixed(2)}
      hint={`grid ${fmtWh(summary.gridChargeWh)} · PV ${fmtWh(summary.pvChargeWh)}`}
    >
      {#snippet icon()}<BatteryMedium class="size-3" />{/snippet}
    </StatTile>
  </div>
</div>
