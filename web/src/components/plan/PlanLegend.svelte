<script lang="ts">
  import type { PlanLayers } from '../../lib/chart/planChart';
  import { themeFor } from '../../lib/chart/theme';
  import { scheme } from '../../lib/state/scheme.svelte';

  interface Props {
    layers: PlanLayers;
  }

  let { layers }: Props = $props();

  const theme = $derived(themeFor(scheme.current));
  const items = $derived([
    { label: 'PV', color: theme.pv, on: layers.forecast, dash: false },
    { label: 'Load', color: theme.load, on: layers.forecast, dash: true },
    { label: 'Battery', color: theme.battery, on: layers.battery, dash: false },
    { label: 'SoC (kWh)', color: theme.soc, on: layers.soc, dash: false },
    { label: 'Target SoC', color: theme.target, on: layers.soc, dash: true },
  ]);
</script>

<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
  {#each items as item (item.label)}
    <span
      class={[
        'flex items-center gap-1 text-[10px]',
        item.on ? 'text-muted' : 'text-faint opacity-40',
      ]}
    >
      <span
        class="inline-block h-[2px] w-3 rounded-full"
        style="background:{item.dash
          ? `repeating-linear-gradient(90deg, ${item.color} 0 2px, transparent 2px 4px)`
          : item.color}"
      ></span>
      {item.label}
    </span>
  {/each}
</div>
