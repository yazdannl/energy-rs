<script lang="ts">
  import BatteryMedium from '@lucide/svelte/icons/battery-medium';
  import HeartPulse from '@lucide/svelte/icons/heart-pulse';
  import Zap from '@lucide/svelte/icons/zap';

  import type { Pack } from '../../lib/api/types';
  import { fmtKwh, fmtW } from '../../lib/format/energy';
  import { packLabel } from '../../lib/format/pack';
  import { ago } from '../../lib/format/time';
  import { batteryFlow, flowVerb } from '../../lib/format/sign';
  import Badge from '../ui/Badge.svelte';
  import SocRing from '../ui/SocRing.svelte';
  import LatchDetail from './LatchDetail.svelte';
  import LearningDetail from './LearningDetail.svelte';
  import OverrideControls from './OverrideControls.svelte';
  import PackActions from './PackActions.svelte';

  interface Props {
    pack: Pack;
    onchanged: () => void;
  }

  let { pack, onchanged }: Props = $props();

  const flow = $derived(batteryFlow(pack.state?.powerW ?? null));
</script>

<article class="card p-3.5">
  <header class="flex items-start justify-between gap-2">
    <div class="min-w-0">
      <h2 class="truncate text-[13px] font-semibold text-ink">{packLabel(pack)}</h2>
      <p class="truncate font-mono text-[10px] text-faint">{pack.sn}</p>
    </div>
    <div class="flex shrink-0 flex-wrap justify-end gap-1">
      {#if pack.state?.stale}<Badge tone="warn">stale</Badge>{/if}
      {#if pack.latch?.blocked}<Badge tone="bad">protected</Badge>{/if}
      {#if pack.override}<Badge tone="info">override</Badge>{/if}
    </div>
  </header>

  <div class="mt-3 flex items-center gap-3">
    <SocRing pct={pack.state?.socPct ?? null} size={70} label="SoC" />
    <div class="min-w-0 flex-1 space-y-1">
      <p class="flex items-baseline gap-1.5">
        <span class="num text-[16px] font-semibold text-ink"
          >{fmtW(pack.state?.powerW ?? null)}</span
        >
        <span class="text-[10.5px] text-muted">{flowVerb(flow)}</span>
      </p>
      <p class="flex items-center gap-1 text-[10px] text-faint">
        <BatteryMedium class="size-3" aria-hidden="true" />
        label {fmtKwh(pack.labelCapacityKwh)} · reported {fmtKwh(pack.reportedCapacityKwh)}
      </p>
      <p class="flex items-center gap-1 text-[10px] text-faint">
        <Zap class="size-3" aria-hidden="true" />
        limits {fmtW(pack.limits.maxChargeW)} / {fmtW(pack.limits.maxDischargeW)}
      </p>
      <p class="flex items-center gap-1 text-[10px] text-faint">
        <HeartPulse class="size-3" aria-hidden="true" />
        sampled {ago(pack.state?.sampledAt ?? null)} · seen {ago(pack.lastSeenAt)}
      </p>
    </div>
  </div>

  <div class="mt-3 border-t border-line pt-3">
    {#if pack.learned}
      <LearningDetail learned={pack.learned} />
    {:else}
      <p class="text-[11px] text-faint">No learning result yet.</p>
    {/if}
  </div>

  <div class="mt-3">
    {#if pack.latch}
      <LatchDetail latch={pack.latch} />
    {:else}
      <p class="text-[11px] text-faint">No protection latch from the engine.</p>
    {/if}
  </div>

  <div class="mt-3 border-t border-line pt-3">
    <OverrideControls {pack} {onchanged} />
  </div>

  <div class="mt-3 border-t border-line pt-3">
    <PackActions {pack} {onchanged} />
  </div>
</article>
