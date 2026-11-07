<script lang="ts">
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';

  import type { SitePack } from '../../lib/api/types';
  import { fmtKwh, fmtW } from '../../lib/format/energy';
  import { ago } from '../../lib/format/time';
  import Badge from '../ui/Badge.svelte';
  import EmptyNote from '../ui/EmptyNote.svelte';

  interface Props {
    packs: SitePack[];
    configured: boolean;
    loading: boolean;
  }

  let { packs, configured, loading }: Props = $props();

  function seenAgo(pack: SitePack): string {
    return ago(pack.lastSeenAt);
  }
</script>

{#if loading && packs.length === 0}
  <div class="space-y-2">
    <div class="h-12 animate-pulse rounded-lg bg-line/60"></div>
    <div class="h-12 animate-pulse rounded-lg bg-line/60"></div>
  </div>
{:else if packs.length === 0}
  <EmptyNote
    title="No packs discovered"
    detail={configured
      ? 'The energy dashboard is configured, but no pack has reported yet.'
      : 'There is no energy dashboard to discover packs from.'}
  />
{:else}
  <ul class="space-y-2">
    {#each packs as pack (pack.sn)}
      <li class="rounded-lg border border-line bg-panel-soft/50 px-2.5 py-2">
        <div class="flex items-start justify-between gap-2">
          <div class="min-w-0">
            <p class="truncate text-[11.5px] font-medium text-ink">{pack.name ?? 'unnamed pack'}</p>
            <p class="truncate font-mono text-[10px] text-faint">serial {pack.sn}</p>
          </div>
          <BatteryCharging class="size-3.5 shrink-0 text-batt" aria-hidden="true" />
        </div>
        <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-muted">
          <span>label {fmtKwh(pack.labelCapacityKwh)}</span>
          <span>reported {fmtKwh(pack.reportedCapacityKwh)}</span>
          <span>{fmtW(pack.maxChargeW)} / {fmtW(pack.maxDischargeW)}</span>
        </div>
        <div class="mt-1 flex flex-wrap items-center gap-1.5">
          <Badge
            tone={seenAgo(pack).endsWith('s ago') || seenAgo(pack) === 'just now' ? 'good' : 'warn'}
          >
            last seen {seenAgo(pack)}
          </Badge>
          <span class="text-[9.5px] text-faint">first seen {ago(pack.firstSeenAt)}</span>
        </div>
      </li>
    {/each}
  </ul>
  <p class="mt-1.5 text-[9.5px] leading-relaxed text-faint">
    The v1 contract exposes the serial as the pack identity; a device address is not part of it, so
    none is shown. Discovery status comes from `lastSeenAt` and the live sample's stale flag.
  </p>
{/if}
