<script lang="ts">
  import type { PlanSlot } from '../../lib/api/types';
  import { fmtEurPerKwh } from '../../lib/format/money';
  import { fmtPct, fmtW, fmtWh } from '../../lib/format/energy';
  import { clock } from '../../lib/format/time';

  interface Props {
    slot: PlanSlot | null;
    pinned: boolean;
  }

  let { slot, pinned }: Props = $props();
</script>

{#if slot === null}
  <p class="text-[11px] text-faint">Hover the chart to read a slot.</p>
{:else}
  <div class="flex flex-wrap items-baseline gap-x-4 gap-y-1 text-[11px]">
    <span class="num rounded bg-ink/[0.06] px-1.5 py-0.5 text-[10.5px] text-ink">
      {pinned ? 'cursor' : 'now'} · {clock(slot.startAt)}
    </span>
    <span class="text-faint"
      >buy <span class="num text-ink">{fmtEurPerKwh(slot.buyEurPerKwh)}</span></span
    >
    <span class="text-faint"
      >feed-in <span class="num text-muted">{fmtEurPerKwh(slot.feedInEurPerKwh)}</span></span
    >
    <span class="text-faint"
      >battery <span class="num text-batt">{fmtW(slot.batteryPowerW)}</span></span
    >
    <span class="text-faint"
      >SoC <span class="num text-ink">{fmtWh(slot.socWh)}</span>{#if slot.socPct !== null}<span
          class="num text-muted"
        >
          · {fmtPct(slot.socPct)}</span
        >{/if}</span
    >
    <!-- The chart plots a slot's average power, so the readout shows the same: a 141 Wh
         quarter-hour is 564 W on the axis, and printing the energy made the two disagree. -->
    <span class="text-faint">PV <span class="num text-pv">{fmtW(slot.pvWh * 4)}</span></span>
    <span class="text-faint">load <span class="num text-muted">{fmtW(slot.loadWh * 4)}</span></span>
    {#if slot.reason}
      <span class="rounded bg-ink/[0.05] px-1.5 py-0.5 text-[10px] text-muted">{slot.reason}</span>
    {/if}
  </div>
{/if}
