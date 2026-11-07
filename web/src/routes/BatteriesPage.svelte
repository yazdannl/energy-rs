<script lang="ts">
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';
  import Database from '@lucide/svelte/icons/database';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { onMount } from 'svelte';

  import { ago } from '../lib/format/time';
  import { fmtW } from '../lib/format/energy';
  import { api } from '../lib/state/runtime';
  import { poll } from '../lib/state/poller';
  import { resource } from '../lib/state/resource.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import PackCard from '../components/batteries/PackCard.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import EmptyNote from '../components/ui/EmptyNote.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';

  const packs = resource(() => api.packs());
  const site = resource(() => api.site());
  const dispatch = resource(() => api.dispatch(20));

  const reloadPacks = () => {
    void packs.load();
    void dispatch.load();
  };

  onMount(() => {
    void packs.load();
    void site.load();
    void dispatch.load();
    const stopPoll = poll(() => {
      void packs.load();
      void dispatch.load();
    }, 60_000);
    return () => {
      stopPoll();
      packs.dispose();
      site.dispose();
      dispatch.dispose();
    };
  });
</script>

<PageHeader
  title="Batteries"
  summary="Live packs, what the learner believes, and what the protection latches hold"
>
  {#snippet actions()}
    {#if site.data}
      <Badge tone={site.data.configured ? 'good' : 'warn'}>
        {site.data.configured ? 'energy dashboard found' : 'no energy dashboard'}
      </Badge>
    {/if}
    <Button onclick={reloadPacks} title="Reload packs and dispatch log">
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      Reload
    </Button>
  {/snippet}
</PageHeader>

{#if packs.error}
  <div class="mt-4"><ErrorNote error={packs.error} onretry={() => void packs.load()} /></div>
{:else if packs.loading && packs.data === null}
  <div class="mt-4 grid gap-3 lg:grid-cols-2">
    <div class="card p-4"><Skeleton rows={8} /></div>
    <div class="card p-4"><Skeleton rows={8} /></div>
  </div>
{:else if (packs.data?.packs.length ?? 0) === 0}
  <div class="mt-4">
    <EmptyNote
      title="No packs in the ledger"
      detail="Discovery records a pack the first time Home Assistant reports one. Nothing is assumed before that."
    />
  </div>
{:else}
  <div class="mt-4 grid gap-3 lg:grid-cols-2 2xl:grid-cols-3">
    {#each packs.data?.packs ?? [] as pack (pack.sn)}
      <PackCard {pack} onchanged={reloadPacks} />
    {/each}
  </div>
{/if}

<div class="mt-3 grid gap-3 xl:grid-cols-2">
  <Panel title="Discovery" subtitle="Sources the engine reads">
    {#snippet icon()}<Database class="size-4 text-grid" />{/snippet}
    {#if site.error}
      <ErrorNote error={site.error} onretry={() => void site.load()} />
    {:else}
      <ul class="space-y-1.5">
        {#each site.data?.sources ?? [] as source (source.entityId)}
          <li
            class="flex items-center justify-between gap-2 rounded-md border border-line/70 px-2 py-1.5"
          >
            <span class="min-w-0">
              <span class="block text-[11px] text-ink">{source.label ?? source.kind}</span>
              <span class="block truncate font-mono text-[10px] text-faint">{source.entityId}</span>
            </span>
            <Badge tone="neutral">{source.kind}</Badge>
          </li>
        {/each}
      </ul>
      {#if (site.data?.sources.length ?? 0) === 0}
        <p class="text-[11px] text-faint">No sources reported.</p>
      {/if}
    {/if}
  </Panel>

  <Panel title="Dispatch log" subtitle="Last 7 days, newest first">
    {#snippet icon()}<BatteryCharging class="size-4 text-batt" />{/snippet}
    {#if dispatch.error}
      <ErrorNote error={dispatch.error} onretry={() => void dispatch.load()} />
    {:else if (dispatch.data?.entries.length ?? 0) === 0}
      <EmptyNote
        title="No dispatch attempts"
        detail="The engine has not tried to write to a pack in this window."
      />
    {:else}
      <div class="max-h-[280px] overflow-y-auto">
        <table class="w-full border-collapse text-[11px]">
          <thead
            class="sticky top-0 bg-panel text-left text-[10px] uppercase tracking-[0.06em] text-faint"
          >
            <tr>
              <th class="py-1 font-medium">When</th>
              <th class="py-1 font-medium">Pack</th>
              <th class="py-1 text-right font-medium">Set / applied</th>
              <th class="py-1 text-right font-medium">Check</th>
            </tr>
          </thead>
          <tbody>
            {#each dispatch.data?.entries ?? [] as entry (entry.at + entry.packSn)}
              <tr class="border-t border-line/60">
                <td class="py-1 text-muted" title={entry.at}>{ago(entry.at)}</td>
                <td class="py-1 font-mono text-[10px] text-ink">{entry.packSn}</td>
                <td class="num py-1 text-right text-ink">
                  {fmtW(entry.requestedW)} / {fmtW(entry.appliedW)}
                </td>
                <td class="py-1 text-right">
                  {#if entry.error}
                    <Badge tone="bad">{entry.reason}</Badge>
                  {:else if entry.verified}
                    <Badge tone="good">verified</Badge>
                  {:else}
                    <Badge tone="warn">{entry.reason}</Badge>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </Panel>
</div>

<p class="mt-2 text-[10px] leading-relaxed text-faint">
  Pack addresses are not part of the v1 contract; the serial is the only identity the API exposes. A
  stale sample is shown as stale, never as a fresh measurement.
</p>
