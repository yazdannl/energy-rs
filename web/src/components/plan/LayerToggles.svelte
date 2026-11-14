<script lang="ts">
  import type { PlanLayers } from '../../lib/chart/planChart';
  import ToggleSwitch from '../ui/ToggleSwitch.svelte';

  interface Props {
    layers: PlanLayers;
    onchange: (layers: PlanLayers) => void;
  }

  let { layers, onchange }: Props = $props();

  const set = (key: keyof PlanLayers) => (value: boolean) => onchange({ ...layers, [key]: value });
</script>

<div class="flex flex-wrap items-center gap-x-4 gap-y-2">
  <ToggleSwitch checked={layers.prices} label="Prices" onchange={set('prices')} />
  <ToggleSwitch checked={layers.forecast} label="PV / load" onchange={set('forecast')} />
  <ToggleSwitch checked={layers.battery} label="Battery" onchange={set('battery')} />
  <ToggleSwitch checked={layers.soc} label="SoC" onchange={set('soc')} />
</div>
