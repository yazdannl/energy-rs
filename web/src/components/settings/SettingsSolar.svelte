<script lang="ts">
  import Sun from '@lucide/svelte/icons/sun';

  import type { SettingsDraft } from '../../lib/state/settingsDraft.svelte';

  interface Props {
    draft: SettingsDraft;
  }

  let { draft }: Props = $props();

  const FIELDS = [
    { key: 'pv_dc_kwp', label: 'Array DC', unit: 'kWp', hint: 'nameplate of the modules' },
    { key: 'pv_ac_kwp', label: 'Inverter AC', unit: 'kW', hint: 'clipping the model may apply' },
    { key: 'pv_tilt_deg', label: 'Tilt', unit: '°', hint: '0 flat, 90 vertical' },
    { key: 'pv_azimuth_deg', label: 'Azimuth', unit: '°', hint: '0 north, 180 south' },
    { key: 'pv_scale', label: 'Model scale', unit: '×', hint: 'calibrates the model to the array' },
  ];

  const modes = $derived(draft.entry('pv_mode')?.options ?? ['auto', 'model', 'provider']);
  const mode = $derived(draft.text('pv_mode', 'auto'));

  function update(key: string, event: Event): void {
    const raw = (event.currentTarget as HTMLInputElement).value;
    const value = raw === '' ? null : Number(raw);
    draft.set(key, value === null || Number.isNaN(value) ? null : value);
  }
</script>

<div class="space-y-2.5">
  <div class="flex items-start gap-2">
    <Sun class="mt-0.5 size-3.5 text-pv" />
    <p class="text-[10px] leading-relaxed text-faint">
      The learned model is served from Open-Meteo weather over Home Assistant's coordinates. In
      <span class="font-mono">auto</span> it runs when the array is configured, and Home Assistant's provider
      forecast takes over when it cannot.
    </p>
  </div>

  <label class="block">
    <span class="text-[11px] text-muted">PV forecast</span>
    <select
      value={mode}
      onchange={(event) => draft.set('pv_mode', (event.currentTarget as HTMLSelectElement).value)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
    >
      {#each modes as option (option)}
        <option value={option}>{option}</option>
      {/each}
    </select>
  </label>

  {#each FIELDS as field (field.key)}
    <label class="block">
      <span class="text-[11px] text-muted">{field.label}</span>
      <span class="ml-1 text-[10px] text-faint">{field.unit}</span>
      <input
        type="number"
        step="0.01"
        value={draft.text(field.key)}
        onchange={(event) => update(field.key, event)}
        class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
      />
      <span class="mt-0.5 block text-[10px] text-faint">{field.hint}</span>
    </label>
  {/each}
</div>
