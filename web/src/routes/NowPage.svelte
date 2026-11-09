<script lang="ts">
  import Activity from '@lucide/svelte/icons/activity';
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';
  import Eye from '@lucide/svelte/icons/eye';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import TriangleAlert from '@lucide/svelte/icons/triangle-exclamation-point';
  import { onMount, untrack } from 'svelte';

  import type { PlanSlot } from '../lib/api/types';
  import { ago } from '../lib/format/time';
  import { api } from '../lib/state/runtime';
  import { connection } from '../lib/state/connection.svelte';
  import { poll } from '../lib/state/poller';
  import { resource } from '../lib/state/resource.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import CurrentSlot from '../components/now/CurrentSlot.svelte';
  import EnergyFlow from '../components/now/EnergyFlow.svelte';
  import ModeSwitch from '../components/ui/ModeSwitch.svelte';
  import PackRings from '../components/now/PackRings.svelte';
  import TodayMoney from '../components/now/TodayMoney.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';

  const plan = resource(() => api.currentPlan());
  const packs = resource(() => api.packs());
  const today = resource(() => api.savings('day'));
  let now = $state(Date.now());

  onMount(() => {
    void plan.load();
    void packs.load();
    void today.load();
    const clock = setInterval(() => (now = Date.now()), 15_000);
    const stopPoll = poll(() => {
      void packs.load();
      void today.load();
    }, 120_000);
    return () => {
      clearInterval(clock);
      stopPoll();
      plan.dispose();
      packs.dispose();
      today.dispose();
    };
  });

  // A `plan` event carries only the id; the page re-reads the plan it actually renders.
  $effect(() => {
    const version = connection.planVersion;
    if (version === 0) return;
    untrack(() => void plan.load());
  });

  const slot = $derived.by<PlanSlot | null>(() => {
    const current = plan.data;
    if (current === null) return null;
    return (
      current.slots.find(
        (item) => new Date(item.startAt).getTime() <= now && now < new Date(item.endAt).getTime(),
      ) ?? null
    );
  });

  const livePacks = $derived(connection.snapshot?.packs ?? []);
  const soc = $derived.by(() => {
    const values = livePacks.map((pack) => pack.socPct).filter((value) => value !== null);
    if (values.length === 0) {
      const known = (packs.data?.packs ?? []).map((pack) => pack.state?.socPct ?? null);
      const filtered = known.filter((value): value is number => value !== null);
      return filtered.length === 0 ? null : filtered.reduce((a, b) => a + b, 0) / filtered.length;
    }
    return values.reduce((a, b) => a + b, 0) / values.length;
  });
  const anyStale = $derived(livePacks.some((pack) => pack.stale));

  const refresh = () => {
    void plan.load();
    void packs.load();
    void today.load();
  };
</script>

<PageHeader
  title="Now"
  summary="Live flow, the quarter-hour the engine is following and today’s money"
>
  {#snippet actions()}
    <Badge tone={connection.status === 'live' ? 'good' : 'warn'}>
      {connection.status === 'live' ? 'live' : connection.status}
    </Badge>
    <Button onclick={refresh} title="Reload plan, packs and today">
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      Refresh
    </Button>
  {/snippet}
</PageHeader>

<div class="mt-4 grid items-start gap-3 xl:grid-cols-3">
  <div class="grid items-start gap-3 xl:col-span-2 xl:grid-cols-5">
    <Panel
      class="xl:col-span-3"
      title="Energy flow"
      subtitle={connection.snapshot?.meter
        ? `meter updated ${ago(connection.snapshot.meter.at)}`
        : 'no meter reading in the last snapshot'}
    >
      {#snippet icon()}<Activity class="size-4 text-good" />{/snippet}
      <EnergyFlow meter={connection.snapshot?.meter ?? null} socPct={soc} stale={anyStale} />
    </Panel>

    <Panel
      class="xl:col-span-2"
      title="Current quarter-hour"
      subtitle="What the optimizer decided for this slot"
    >
      {#snippet icon()}<Eye class="size-4 text-batt" />{/snippet}
      {#if plan.loading && plan.data === null}
        <Skeleton rows={4} />
      {:else if plan.error}
        <ErrorNote error={plan.error} onretry={() => void plan.load()} />
      {:else}
        <CurrentSlot {slot} hasPlan={plan.data !== null} />
        {#if plan.data?.summary.zeroExport}
          <p class="mt-3 text-xs text-muted">
            Zero export: the battery never sells; PV surplus still flows to the grid.
          </p>
        {:else if plan.data?.summary.exportFloorEurPerKwh != null}
          <p class="mt-3 text-xs text-muted">
            Selling needs ≥ €{plan.data.summary.exportFloorEurPerKwh.toFixed(3)}/kWh (stored €{(
              plan.data.summary.costBasisEurPerKwh ?? 0
            ).toFixed(3)}, wear €0.020)
            {#if plan.data.summary.gatedWh > 0}
              · kept {(plan.data.summary.gatedWh / 1000).toFixed(2)} kWh in the pack
            {/if}
            {#if plan.data.summary.solarReserveWh > 0}
              · {(plan.data.summary.solarReserveWh / 1000).toFixed(1)} kWh free for solar
            {/if}
          </p>
        {/if}
      {/if}
    </Panel>
  </div>

  <div class="space-y-3">
    <Panel title="Today" subtitle="Measured, not planned">
      {#snippet icon()}<BatteryCharging class="size-4 text-pv" />{/snippet}
      {#if today.loading && today.data === null}
        <Skeleton rows={3} />
      {:else if today.error}
        <ErrorNote error={today.error} onretry={() => void today.load()} />
      {:else}
        <TodayMoney savings={today.data} />
      {/if}
    </Panel>

    <Panel title="Packs" subtitle={`${packs.data?.packs.length ?? 0} in the ledger`}>
      {#if packs.error}
        <ErrorNote error={packs.error} onretry={() => void packs.load()} />
      {:else}
        <PackRings packs={packs.data?.packs ?? []} live={livePacks} loading={packs.loading} />
      {/if}
    </Panel>

    <Panel title="Control mode" subtitle="Advisory, dry run or auto">
      <ModeSwitch mode={connection.snapshot?.mode ?? null} />
    </Panel>

    {#if connection.faults.length > 0}
      <Panel title="Open faults" subtitle="From the last live snapshot">
        {#snippet icon()}<TriangleAlert class="size-4 text-warn" />{/snippet}
        <ul class="space-y-1.5">
          {#each connection.faults as fault (fault.kind)}
            <li class="rounded-lg border border-warn/25 bg-warn/5 px-2.5 py-2">
              <p class="flex items-center justify-between gap-2 text-[11px] font-medium text-warn">
                <span class="truncate">{fault.kind}</span>
                <span class="shrink-0 text-[10px] text-faint">{ago(fault.since)}</span>
              </p>
              <p class="mt-0.5 text-[10.5px] leading-relaxed text-muted">{fault.message}</p>
            </li>
          {/each}
        </ul>
      </Panel>
    {/if}
  </div>
</div>
