<script lang="ts">
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-exclamation-point';

  import type { PackLatch } from '../../lib/api/types';
  import { ago } from '../../lib/format/time';
  import { fmtW } from '../../lib/format/energy';
  import Badge from '../ui/Badge.svelte';

  interface Props {
    latch: PackLatch;
  }

  let { latch }: Props = $props();
</script>

<div class="rounded-lg border border-line bg-panel-soft/50 px-2.5 py-2">
  <div class="flex items-center justify-between gap-2">
    <span
      class="flex items-center gap-1.5 text-[11px] font-medium"
      class:text-warn={latch.blocked}
      class:text-good={!latch.blocked}
    >
      {#if latch.blocked}
        <TriangleAlert class="size-3.5" aria-hidden="true" />
        Protection latch
      {:else}
        <ShieldCheck class="size-3.5" aria-hidden="true" />
        Healthy
      {/if}
    </span>
    {#if latch.forceCharge}
      <Badge tone="warn">force charge {fmtW(latch.forceChargeW)}</Badge>
    {/if}
  </div>
  {#if latch.blocked && latch.reason}
    <p class="mt-1 font-mono text-[10px] text-warn">{latch.reason}</p>
    <p class="text-[10px] text-faint">
      since {ago(latch.since)} · cell {latch.voltageV?.toFixed(2) ?? '—'} V · fresh {ago(
        latch.lastFreshAt,
      )}
    </p>
  {:else}
    <p class="mt-1 text-[10px] text-faint">
      No latch · cell {latch.voltageV?.toFixed(2) ?? '—'} V · last fresh {ago(latch.lastFreshAt)}
    </p>
  {/if}
</div>
