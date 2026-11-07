<script lang="ts">
  import type { LivePack, Pack } from '../../lib/api/types';
  import { fmtW } from '../../lib/format/energy';
  import { flowVerb, batteryFlow } from '../../lib/format/sign';
  import Badge from '../ui/Badge.svelte';
  import EmptyNote from '../ui/EmptyNote.svelte';
  import SocRing from '../ui/SocRing.svelte';

  interface Props {
    packs: Pack[];
    live: LivePack[];
    loading: boolean;
  }

  let { packs, live, loading }: Props = $props();

  const rows = $derived(
    packs.map((pack) => {
      const stream = live.find((item) => item.sn === pack.sn);
      return {
        sn: pack.sn,
        name: pack.name ?? pack.sn,
        socPct: stream?.socPct ?? pack.state?.socPct ?? null,
        powerW: stream?.powerW ?? pack.state?.powerW ?? null,
        stale: stream?.stale ?? pack.state?.stale ?? false,
        blocked: stream?.blocked ?? pack.latch?.blocked ?? false,
      };
    }),
  );
</script>

{#if loading && packs.length === 0}
  <div class="grid grid-cols-2 gap-3">
    <div class="h-[74px] animate-pulse rounded-lg bg-line/60"></div>
    <div class="h-[74px] animate-pulse rounded-lg bg-line/60"></div>
  </div>
{:else if rows.length === 0}
  <EmptyNote title="No packs in the ledger" detail="Discovery has not recorded a pack yet." />
{:else}
  <ul class="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
    {#each rows as row (row.sn)}
      <li class="flex items-center gap-2.5 rounded-lg border border-line bg-panel-soft/50 p-2">
        <SocRing pct={row.socPct} size={58} />
        <div class="min-w-0 flex-1">
          <p class="truncate text-[11.5px] font-medium text-ink">{row.name}</p>
          <p class="num text-[11px] text-muted">
            {fmtW(row.powerW)} · {flowVerb(batteryFlow(row.powerW))}
          </p>
          <div class="mt-1 flex flex-wrap gap-1">
            {#if row.stale}<Badge tone="warn">stale</Badge>{/if}
            {#if row.blocked}<Badge tone="bad">protected</Badge>{/if}
          </div>
        </div>
      </li>
    {/each}
  </ul>
{/if}
