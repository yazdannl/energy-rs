<script lang="ts">
  import BatteryMedium from '@lucide/svelte/icons/battery-medium';
  import House from '@lucide/svelte/icons/house';
  import PlugZap from '@lucide/svelte/icons/plug-zap';
  import Sun from '@lucide/svelte/icons/sun';

  import type { LiveMeter } from '../../lib/api/types';
  import { fmtPct, fmtW } from '../../lib/format/energy';
  import { batteryFlow, flowVerb, gridFlow } from '../../lib/format/sign';

  interface Props {
    meter: LiveMeter | null;
    socPct: number | null;
    stale: boolean;
  }

  let { meter, socPct, stale }: Props = $props();

  const pvW = $derived(meter?.pvW ?? null);
  const gridW = $derived(meter?.gridW ?? null);
  const loadW = $derived(meter?.loadW ?? null);
  const batteryW = $derived(meter?.batteryW ?? null);

  const grid = $derived(gridFlow(gridW));
  const battery = $derived(batteryFlow(batteryW));
  const pvActive = $derived((pvW ?? 0) > 15);

  /** Percent position for a point in the 300 × 200 viewBox (`aspect-[3/2]` keeps it exact). */
  const at = (x: number, y: number) => `left:${(x / 3).toFixed(2)}%;top:${y / 2}%`;

  // The wires run from each satellite's centre to the house's centre and disappear *under* the
  // cards, so no combination of viewport width and card height can open a visible gap.
  const WIRE = {
    pv: 'M 46 38 Q 46 96 150 96',
    gridToHouse: 'M 254 38 Q 254 96 150 96',
    houseToGrid: 'M 150 96 Q 254 96 254 38',
    houseToBattery: 'M 150 96 V 164',
    batteryToHouse: 'M 150 164 V 96',
  };
</script>

<div class="relative mx-auto aspect-[3/2] w-full max-w-[480px]">
  <svg viewBox="0 0 300 200" class="absolute inset-0 h-full w-full" aria-hidden="true">
    <path
      d={WIRE.pv}
      class="fill-none stroke-line-strong"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    {#if pvActive}
      <path
        d={WIRE.pv}
        class="flow-line fill-none stroke-pv"
        stroke-width="2"
        stroke-linecap="round"
      />
    {/if}

    <path
      d={grid === 'export' ? WIRE.houseToGrid : WIRE.gridToHouse}
      class="fill-none stroke-line-strong"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    {#if grid !== 'idle'}
      <path
        d={grid === 'export' ? WIRE.houseToGrid : WIRE.gridToHouse}
        class="flow-line fill-none stroke-grid"
        stroke-width="2"
        stroke-linecap="round"
      />
    {/if}

    <path
      d={battery === 'discharge' ? WIRE.batteryToHouse : WIRE.houseToBattery}
      class="fill-none stroke-line-strong"
      stroke-width="1.6"
      stroke-linecap="round"
    />
    {#if battery !== 'idle'}
      <path
        d={battery === 'discharge' ? WIRE.batteryToHouse : WIRE.houseToBattery}
        class="flow-line fill-none stroke-batt"
        stroke-width="2"
        stroke-linecap="round"
      />
    {/if}
  </svg>

  <div
    class="absolute w-[30%] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line bg-panel px-1.5 py-1.5 text-center"
    style={at(46, 38)}
  >
    <Sun class="mx-auto size-3.5 text-pv" aria-hidden="true" />
    <p class="text-[8.5px] uppercase tracking-[0.08em] text-faint">PV</p>
    <p class="num text-[12px] font-semibold text-ink">{fmtW(pvW)}</p>
  </div>

  <div
    class="absolute w-[30%] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line bg-panel px-1.5 py-1.5 text-center"
    style={at(254, 38)}
  >
    <PlugZap class="mx-auto size-3.5 text-grid" aria-hidden="true" />
    <p class="text-[8.5px] uppercase tracking-[0.08em] text-faint">Grid</p>
    <p class="num text-[12px] font-semibold text-ink">{fmtW(gridW)}</p>
    <p class="text-[8.5px] text-faint">{flowVerb(grid)}</p>
  </div>

  <div
    class="absolute w-[34%] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line-strong bg-panel px-2 py-2 text-center shadow-lg"
    style={at(150, 96)}
  >
    <House class="mx-auto size-4 text-load" aria-hidden="true" />
    <p class="text-[8.5px] uppercase tracking-[0.08em] text-faint">House</p>
    <p class="num text-[13px] font-semibold text-ink">{fmtW(loadW)}</p>
  </div>

  <div
    class="absolute w-[42%] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-line bg-panel px-1.5 py-1.5 text-center"
    style={at(150, 164)}
  >
    <div class="flex items-center justify-center gap-1 whitespace-nowrap">
      <BatteryMedium class="size-3.5 shrink-0 text-batt" aria-hidden="true" />
      <span class="num text-[12px] font-semibold text-ink">{fmtPct(socPct)}</span>
      {#if stale}
        <span class="rounded bg-warn/15 px-1 text-[8px] font-semibold uppercase text-warn"
          >stale</span
        >
      {/if}
    </div>
    <p class="text-[9.5px] whitespace-nowrap text-muted">{fmtW(batteryW)} · {flowVerb(battery)}</p>
  </div>
</div>

<p class="mt-1 text-center text-[10px] text-faint">
  Positive battery power charges · positive grid power imports
</p>
