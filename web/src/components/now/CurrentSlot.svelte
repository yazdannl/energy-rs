<script lang="ts">
  import Clock from '@lucide/svelte/icons/clock';
  import { onMount } from 'svelte';

  import type { PlanSlot } from '../../lib/api/types';
  import { fmtEurPerKwh } from '../../lib/format/money';
  import { fmtW, fmtWh } from '../../lib/format/energy';
  import { clock } from '../../lib/format/time';
  import { batteryFlow, flowVerb } from '../../lib/format/sign';
  import Badge from '../ui/Badge.svelte';
  import EmptyNote from '../ui/EmptyNote.svelte';

  interface Props {
    slot: PlanSlot | null;
    hasPlan: boolean;
  }

  let { slot, hasPlan }: Props = $props();

  let now = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 15_000);
    return () => clearInterval(timer);
  });

  const elapsed = $derived.by(() => {
    if (slot === null) return 0;
    const start = new Date(slot.startAt).getTime();
    const end = new Date(slot.endAt).getTime();
    return Math.max(0, Math.min(100, ((now - start) / (end - start)) * 100));
  });

  const flow = $derived(batteryFlow(slot?.batteryPowerW ?? null));
  // While saldering is on, an exported kWh is worth the all-in import price, so the two lines match.
  const netted = $derived(
    slot !== null &&
      slot.buyEurPerKwh > 0 &&
      Math.abs(slot.feedInEurPerKwh - slot.buyEurPerKwh) < 0.0005,
  );
</script>

{#if slot === null}
  <EmptyNote
    title={hasPlan ? 'The plan does not cover this instant' : 'No plan yet'}
    detail={hasPlan
      ? 'The current instant is outside the stored horizon; the next solve will cover it.'
      : 'The engine has not published a plan. Once it ticks, the current quarter-hour appears here.'}
  />
{:else}
  <div class="space-y-2.5">
    <div class="flex items-center justify-between gap-2">
      <span class="inline-flex items-center gap-1.5 text-[11.5px] text-muted">
        <Clock class="size-3.5 text-faint" aria-hidden="true" />
        <span class="num">{clock(slot.startAt)} – {clock(slot.endAt)}</span>
      </span>
      <div class="flex items-center gap-1.5">
        {#if slot.reason && slot.reason !== 'idle'}
          <Badge tone="info">{slot.reason}</Badge>
        {/if}
        <Badge tone={flow === 'charge' ? 'batt' : flow === 'discharge' ? 'pv' : 'neutral'}>
          {flowVerb(flow)}
        </Badge>
      </div>
    </div>

    <div class="h-[3px] overflow-hidden rounded-full bg-line">
      <div
        class="h-full rounded-full bg-good/70 transition-all duration-700"
        style="width:{elapsed}%"
      ></div>
    </div>

    <dl class="grid grid-cols-2 gap-x-3 gap-y-2">
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">Buy</dt>
        <dd class="num text-[13px] font-medium text-ink">{fmtEurPerKwh(slot.buyEurPerKwh)}</dd>
      </div>
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">Feed-in</dt>
        <dd class="num text-[13px] font-medium text-ink">
          {fmtEurPerKwh(slot.feedInEurPerKwh)}
          {#if netted}
            <span class="text-[9.5px] font-normal text-faint">· netted 1:1</span>
          {/if}
        </dd>
      </div>
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">Planned battery</dt>
        <dd class="num text-[13px] font-medium text-batt">{fmtW(slot.batteryPowerW)}</dd>
      </div>
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">Planned SoC</dt>
        <dd class="num text-[13px] font-medium text-ink">{fmtWh(slot.socWh)}</dd>
      </div>
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">PV / load</dt>
        <dd class="num text-[12px] text-muted">{fmtWh(slot.pvWh)} / {fmtWh(slot.loadWh)}</dd>
      </div>
      <div>
        <dt class="text-[10px] uppercase tracking-[0.07em] text-faint">Target SoC</dt>
        <dd class="num text-[12px] text-muted">{fmtWh(slot.targetSocWh)}</dd>
      </div>
    </dl>
  </div>
{/if}
