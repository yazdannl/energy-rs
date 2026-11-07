<script lang="ts">
  import 'uplot/dist/uPlot.min.css';

  import uPlot from 'uplot';

  import type { Plan, PlanSlot } from '../../lib/api/types';
  import { layerVisibility, planChart, type PlanLayers } from '../../lib/chart/planChart';
  import { themeFor } from '../../lib/chart/theme';
  import { clock } from '../../lib/format/time';
  import { scheme } from '../../lib/state/scheme.svelte';
  import PlanLegend from './PlanLegend.svelte';
  import PlanReadout from './PlanReadout.svelte';

  interface Props {
    plan: Plan;
    layers: PlanLayers;
  }

  let { plan, layers }: Props = $props();

  let host = $state<HTMLDivElement | null>(null);
  let chart = $state<uPlot | null>(null);
  let hovered = $state<number | null>(null);

  const currentSlot = $derived.by<PlanSlot | null>(() => {
    const now = Date.now();
    return (
      plan.slots.find(
        (slot) => new Date(slot.startAt).getTime() <= now && now < new Date(slot.endAt).getTime(),
      ) ??
      plan.slots[0] ??
      null
    );
  });

  const shown = $derived(
    hovered !== null && plan.slots[hovered] !== undefined ? plan.slots[hovered] : currentSlot,
  );

  $effect(() => {
    const element = host;
    const data = plan;
    const theme = themeFor(scheme.current);
    if (element === null) return;
    const model = planChart(data, theme, (index) => {
      hovered = index;
    });
    const instance = new uPlot(
      { ...model.options, width: Math.max(280, element.clientWidth), height: 288 },
      model.data,
      element,
    );
    const observer = new ResizeObserver(() => {
      instance.setSize({ width: Math.max(280, element.clientWidth), height: 288 });
    });
    observer.observe(element);
    chart = instance;
    return () => {
      observer.disconnect();
      instance.destroy();
      chart = null;
    };
  });

  $effect(() => {
    const instance = chart;
    if (instance === null) return;
    for (const [index, visible] of layerVisibility(layers))
      instance.setSeries(index, { show: visible });
  });
</script>

<div class="space-y-2">
  <PlanLegend {layers} />
  <div bind:this={host} class="w-full overflow-hidden"></div>
  <PlanReadout slot={shown} pinned={hovered !== null} />
  <p class="text-[10px] leading-relaxed text-faint">
    SoC is drawn in kWh on a hidden scale (the plan does not carry pack capacity). Battery bars
    above the axis charge; below the axis they discharge. Hover a quarter-hour for exact values.
  </p>
  <p class="sr-only">Plan from {clock(plan.horizonStartAt)} to {clock(plan.horizonEndAt)}</p>
</div>
