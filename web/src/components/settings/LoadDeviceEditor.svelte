<script lang="ts">
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { LoadDevice } from '../../lib/api/types';

  interface Props {
    device: LoadDevice;
    modelled: boolean | undefined;
    onupdate: (patch: Partial<LoadDevice>) => void;
    onremove: () => void;
  }

  let { device, modelled, onupdate, onremove }: Props = $props();

  const KINDS = [
    { value: 'heat_pump', label: 'Heat pump' },
    { value: 'ev_charger', label: 'EV charger' },
    { value: 'water_heater', label: 'Water heater' },
    { value: 'pool_pump', label: 'Pool pump' },
    { value: 'other', label: 'Other meter' },
  ];

  const selected = $derived(
    KINDS.some((kind) => kind.value === device.kind) ? device.kind : 'other',
  );
</script>

<div class="rounded-md border border-line bg-panel/60 p-2">
  <div class="flex items-center gap-1.5">
    <input
      type="text"
      placeholder="sensor.heat_pump_power"
      value={device.entity}
      oninput={(event) => onupdate({ entity: event.currentTarget.value })}
      class="min-w-0 flex-1 rounded border border-line bg-panel px-2 py-1 font-mono text-[11px] text-ink focus:border-line-strong focus:outline-none"
    />
    <button
      type="button"
      aria-label="Remove this load"
      onclick={onremove}
      class="rounded border border-line p-1 text-faint hover:border-line-strong hover:text-ink"
    >
      <Trash2 class="size-3.5" />
    </button>
  </div>
  <div class="mt-2 grid gap-2 sm:grid-cols-2">
    <label class="block">
      <span class="text-[10px] text-faint">Kind</span>
      <select
        value={selected}
        onchange={(event) => onupdate({ kind: event.currentTarget.value })}
        class="mt-0.5 w-full rounded border border-line bg-panel px-2 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
      >
        {#each KINDS as kind (kind.value)}
          <option value={kind.value}>{kind.label}</option>
        {/each}
      </select>
    </label>
    <label class="block">
      <span class="text-[10px] text-faint">Name</span>
      <input
        type="text"
        placeholder="Heat pump"
        value={device.name}
        oninput={(event) => onupdate({ name: event.currentTarget.value })}
        class="mt-0.5 w-full rounded border border-line bg-panel px-2 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
      />
    </label>
  </div>
  <label class="mt-2 flex items-center gap-2 text-[10px] text-faint">
    <input
      type="checkbox"
      checked={device.keepWhileAway}
      onchange={(event) => onupdate({ keepWhileAway: event.currentTarget.checked })}
      class="size-3 accent-current"
    />
    Runs while away (kept in holiday mode)
    {#if device.entity.trim() !== '' && modelled === false}
      <span class="text-warn">· not modelled yet</span>
    {:else if device.entity.trim() !== ''}
      <span class="text-good">· modelled</span>
    {/if}
  </label>
</div>
