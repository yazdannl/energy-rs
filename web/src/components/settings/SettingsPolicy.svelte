<script lang="ts">
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';

  import type { SettingsDraft } from '../../lib/state/settingsDraft.svelte';

  interface Props {
    draft: SettingsDraft;
  }

  let { draft }: Props = $props();

  const FIELDS = [
    {
      key: 'solar_reserve_pct',
      label: 'Solar reserve',
      unit: '%',
      hint: 'kept free of grid energy',
    },
    { key: 'peak_reserve_pct', label: 'Peak reserve', unit: '%', hint: 'guideline, never a floor' },
    { key: 'wear_eur_per_kwh', label: 'Wear', unit: '€/kWh', hint: 'per delivered kWh' },
    {
      key: 'peak_reserve_value_eur_per_kwh',
      label: 'Headroom value',
      unit: '€/kWh',
      hint: 'when to release the peak reserve',
    },
    {
      key: 'cost_basis_eur_per_kwh',
      label: 'Cost basis',
      unit: '€/kWh',
      hint: '0 = from measurements',
    },
  ];

  const policies = $derived(draft.entry('export_policy')?.options ?? ['legacy', 'cost_basis']);
  const zeroExport = $derived(draft.text('zero_export') === 'on');

  function update(key: string, event: Event): void {
    const raw = (event.currentTarget as HTMLInputElement).value;
    const value = raw === '' ? null : Number(raw);
    draft.set(key, value === null || Number.isNaN(value) ? null : value);
  }
</script>

<div class="space-y-2.5">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-start gap-2">
      <BatteryCharging class="mt-0.5 size-3.5 text-batt" />
      <div>
        <p class="text-[11px] text-muted">Zero export</p>
        <p class="text-[10px] text-faint">The battery never sells; PV surplus still flows.</p>
      </div>
    </div>
    <button
      type="button"
      role="switch"
      aria-checked={zeroExport}
      aria-label="Zero export"
      onclick={() => draft.set('zero_export', zeroExport ? 'off' : 'on')}
      class="relative h-5 w-9 shrink-0 rounded-full border transition-colors {zeroExport
        ? 'border-good bg-good/30'
        : 'border-line bg-panel'}"
    >
      <span
        class="absolute top-0.5 size-3.5 rounded-full transition-all {zeroExport
          ? 'left-4.5 bg-good'
          : 'left-0.5 bg-line-strong'}"
      ></span>
    </button>
  </div>

  <label class="block">
    <span class="text-[11px] text-muted">Export policy</span>
    <select
      value={draft.text('export_policy')}
      onchange={(event) =>
        draft.set('export_policy', (event.currentTarget as HTMLSelectElement).value)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
    >
      {#each policies as option (option)}
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
        step="0.001"
        value={draft.text(field.key)}
        onchange={(event) => update(field.key, event)}
        class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
      />
      <span class="mt-0.5 block text-[10px] text-faint">{field.hint}</span>
    </label>
  {/each}
</div>
