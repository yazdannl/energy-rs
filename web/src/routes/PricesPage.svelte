<script lang="ts">
  import Euro from '@lucide/svelte/icons/euro';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Sun from '@lucide/svelte/icons/sun';
  import { onMount } from 'svelte';

  import { fmtEurPerKwh } from '../lib/format/money';
  import { clock } from '../lib/format/time';
  import { api } from '../lib/state/runtime';
  import { poll } from '../lib/state/poller';
  import { resource } from '../lib/state/resource.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import PriceHeatmap from '../components/prices/PriceHeatmap.svelte';
  import PriceTable from '../components/prices/PriceTable.svelte';
  import SettlementPreview from '../components/prices/SettlementPreview.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import EmptyNote from '../components/ui/EmptyNote.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';
  import StatTile from '../components/ui/StatTile.svelte';

  const prices = resource(() => api.prices());

  onMount(() => {
    void prices.load();
    const stopPoll = poll(() => void prices.load(), 600_000);
    return () => {
      stopPoll();
      prices.dispose();
    };
  });

  const slots = $derived(prices.data?.slots ?? []);
  const buys = $derived(slots.map((slot) => slot.buyEurPerKwh));
  const min = $derived(buys.length === 0 ? null : Math.min(...buys));
  const max = $derived(buys.length === 0 ? null : Math.max(...buys));
  const avg = $derived(
    buys.length === 0
      ? null
      : Math.round((buys.reduce((a, b) => a + b, 0) / buys.length) * 1e6) / 1e6,
  );
  const negative = $derived(buys.filter((value) => value < 0).length);
  const source = $derived(slots[0]?.source ?? null);
</script>

<PageHeader
  title="Prices"
  summary="Published quarter-hour slots, and what an export would be worth"
>
  {#snippet actions()}
    {#if source}<Badge tone="neutral">{source}</Badge>{/if}
    <Button onclick={() => void prices.load()} title="Reload prices">
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      Reload
    </Button>
  {/snippet}
</PageHeader>

{#if prices.loading && prices.data === null}
  <div class="mt-4 space-y-3">
    <div class="card p-4"><Skeleton rows={3} /></div>
    <div class="card p-4"><Skeleton rows={6} height="h-6" /></div>
  </div>
{:else if prices.error}
  <div class="mt-4"><ErrorNote error={prices.error} onretry={() => void prices.load()} /></div>
{:else if slots.length === 0}
  <div class="mt-4">
    <EmptyNote
      title="No published prices"
      detail="The provider has not published slots for this window. A gap inside stored data is never filled with a guess."
    />
  </div>
{:else}
  <div class="mt-4 space-y-3">
    <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
      <StatTile label="Cheapest" value={fmtEurPerKwh(min)} tone="good">
        {#snippet icon()}<Euro class="size-3" />{/snippet}
      </StatTile>
      <StatTile label="Dearest" value={fmtEurPerKwh(max)} tone="bad">
        {#snippet icon()}<Euro class="size-3" />{/snippet}
      </StatTile>
      <StatTile label="Average" value={fmtEurPerKwh(avg)} />
      <StatTile
        label="Below zero"
        value={String(negative)}
        unit="slots"
        hint={negative > 0 ? 'import pays you' : 'no slot below zero'}
        tone={negative > 0 ? 'good' : 'neutral'}
      >
        {#snippet icon()}<Sun class="size-3" />{/snippet}
      </StatTile>
    </div>

    <Panel
      title="Next 48 hours"
      subtitle={`${slots.length} slots · ${clock(prices.data?.from)} → ${clock(prices.data?.to)} local`}
    >
      {#snippet icon()}<Euro class="size-4 text-pv" />{/snippet}
      <PriceHeatmap {slots} />
    </Panel>

    <div class="grid items-start gap-3 xl:grid-cols-2">
      <Panel title="Hourly list" subtitle="Buy range per hour, as billed">
        <PriceTable {slots} />
      </Panel>

      <Panel title="Settlement preview" subtitle="Feed-in per slot under both regimes">
        <SettlementPreview {slots} />
      </Panel>
    </div>
  </div>
{/if}
