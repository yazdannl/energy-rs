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
  let resetView = $state<(() => void) | null>(null);

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

    // Zoom with the wheel and pan by dragging, both clamped to the plan's own horizon. uPlot's
    // scale is in epoch seconds, so a pixel is worth `span / width` seconds.
    const [fullFrom, fullTo] = [model.data[0][0], model.data[0][model.data[0].length - 1]];
    const oneSlot = 15 * 60;
    const current = (): [number, number] => {
      const min = instance.scales.x.min ?? fullFrom;
      const max = instance.scales.x.max ?? fullTo;
      return [Math.max(fullFrom, min), Math.min(fullTo, max)];
    };
    const apply = (from: number, to: number) => {
      const span = Math.max(oneSlot, Math.min(fullTo - fullFrom, to - from));
      let start = Math.max(fullFrom, Math.min(fullTo - span, from));
      const end = start + span;
      if (end > fullTo) start = fullTo - span;
      instance.setScale('x', { min: start, max: start + span });
    };
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const [from, to] = current();
      const center = instance.posToVal(event.offsetX, 'x');
      const factor = event.deltaY > 0 ? 1.3 : 1 / 1.3;
      const span = Math.min(fullTo - fullFrom, (to - from) * factor);
      apply(
        center - (span * (center - from)) / (to - from),
        center + (span * (to - center)) / (to - from),
      );
    };
    let dragging: { x: number; from: number; to: number } | null = null;
    const onPointerDown = (event: PointerEvent) => {
      const [from, to] = current();
      if (to - from >= fullTo - fullFrom) return;
      dragging = { x: event.clientX, from, to };
      element.setPointerCapture(event.pointerId);
    };
    const onPointerMove = (event: PointerEvent) => {
      if (dragging === null) return;
      const width = instance.over.clientWidth || 1;
      const seconds = ((event.clientX - dragging.x) / width) * (dragging.to - dragging.from);
      apply(dragging.from - seconds, dragging.to - seconds);
    };
    const onPointerUp = () => {
      dragging = null;
    };
    const onDoubleClick = () => apply(fullFrom, fullTo);
    const over = instance.over;
    over.addEventListener('wheel', onWheel, { passive: false });
    over.addEventListener('pointerdown', onPointerDown);
    over.addEventListener('pointermove', onPointerMove);
    over.addEventListener('pointerup', onPointerUp);
    over.addEventListener('pointercancel', onPointerUp);
    over.addEventListener('dblclick', onDoubleClick);
    resetView = () => apply(fullFrom, fullTo);

    chart = instance;
    return () => {
      observer.disconnect();
      over.removeEventListener('wheel', onWheel);
      over.removeEventListener('pointerdown', onPointerDown);
      over.removeEventListener('pointermove', onPointerMove);
      over.removeEventListener('pointerup', onPointerUp);
      over.removeEventListener('pointercancel', onPointerUp);
      over.removeEventListener('dblclick', onDoubleClick);
      instance.destroy();
      chart = null;
      resetView = null;
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
  <div class="relative">
    <div bind:this={host} class="w-full touch-none overflow-hidden select-none"></div>
    <button
      type="button"
      class="absolute top-1 right-1 rounded border border-line bg-panel/80 px-1.5 py-0.5 text-[9.5px] text-muted"
      onclick={() => resetView?.()}
    >
      reset view
    </button>
  </div>
  <PlanReadout slot={shown} pinned={hovered !== null} />
  <p class="text-[10px] leading-relaxed text-faint">
    SoC is drawn in kWh on a hidden scale (the plan does not carry pack capacity). Battery bars
    above the axis charge; below the axis they discharge. Hover a quarter-hour for exact values.
  </p>
  <p class="sr-only">Plan from {clock(plan.horizonStartAt)} to {clock(plan.horizonEndAt)}</p>
</div>
