<script lang="ts">
  import type { SettingsDraft } from '../../lib/state/settingsDraft.svelte';

  interface Props {
    draft: SettingsDraft;
  }

  let { draft }: Props = $props();

  const FIELDS = [
    { key: 'grid_import_limit_w', label: 'Grid import limit', unit: 'W', hint: '0 = no limit' },
    { key: 'grid_export_limit_w', label: 'Grid export limit', unit: 'W', hint: '0 = no limit' },
    { key: 'reserve_soc_pct', label: 'Reserve SoC', unit: '%', hint: 'never planned below this' },
    { key: 'max_soc_pct', label: 'Maximum SoC', unit: '%', hint: 'never planned above this' },
    {
      key: 'battery_power_limit_w',
      label: 'Fleet power limit',
      unit: 'W',
      hint: 'charge and discharge',
    },
    {
      key: 'min_pack_power_w',
      label: 'Smallest pack start',
      unit: 'W',
      hint: 'below this one pack is not worth starting',
    },
    {
      key: 'grid_follow_deadband_w',
      label: 'Grid follow deadband',
      unit: 'W',
      hint: 'smaller import errors are left alone',
    },
    { key: 'replan_seconds', label: 'Replan interval', unit: 's', hint: 'solve cadence' },
    {
      key: 'stale_input_seconds',
      label: 'Stale input',
      unit: 's',
      hint: 'hold after this silence',
    },
  ];

  function update(key: string, event: Event): void {
    const raw = (event.currentTarget as HTMLInputElement).value;
    draft.set(key, raw === '' ? null : Number(raw));
  }
</script>

<div class="grid gap-2.5 sm:grid-cols-2">
  {#each FIELDS as field (field.key)}
    {@const entry = draft.entry(field.key)}
    <label class="block">
      <span class="flex items-baseline justify-between gap-2">
        <span class="text-[11px] text-muted">{field.label}</span>
        <span class="text-[9.5px] text-faint">{field.unit}</span>
      </span>
      <input
        type="number"
        value={draft.number(field.key)}
        min={entry?.min}
        max={entry?.max}
        step="1"
        oninput={(event) => update(field.key, event)}
        class="num mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
      />
      <span class="mt-0.5 block text-[9.5px] text-faint">
        {field.hint}
        {#if entry?.min !== undefined && entry.max !== undefined}
          · {entry.min}–{entry.max}
        {/if}
      </span>
    </label>
  {/each}
</div>

<div class="mt-3 border-t border-line pt-3">
  <p class="text-[10px] uppercase tracking-[0.07em] text-faint">Weekly full charge</p>
  <div class="mt-1.5 grid gap-2.5 sm:grid-cols-2">
    <label class="block">
      <span class="text-[11px] text-muted">Weekday</span>
      <select
        value={draft.text('full_charge_weekday')}
        onchange={(event) =>
          draft.set('full_charge_weekday', (event.currentTarget as HTMLSelectElement).value)}
        class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
      >
        {#each ['monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday', 'sunday'] as day (day)}
          <option value={day}>{day}</option>
        {/each}
      </select>
    </label>
    <label class="block">
      <span class="text-[11px] text-muted">Time</span>
      <input
        type="time"
        value={draft.text('full_charge_time')}
        oninput={(event) =>
          draft.set('full_charge_time', (event.currentTarget as HTMLInputElement).value)}
        class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
      />
    </label>
  </div>
  <p class="mt-1 text-[9.5px] text-faint">
    The weekly top-up to 100 % is a protection habit, not a price decision.
  </p>
</div>
