<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import Sun from '@lucide/svelte/icons/sun';
  import Trash2 from '@lucide/svelte/icons/trash-2';

  import type { SettingsDraft } from '../../lib/state/settingsDraft.svelte';

  interface Props {
    draft: SettingsDraft;
  }

  interface Plane {
    name: string;
    kwp: number;
    tilt: number;
    azimuth: number;
  }

  let { draft }: Props = $props();

  const modes = $derived(draft.entry('pv_mode')?.options ?? ['auto', 'model', 'provider']);
  const mode = $derived(draft.text('pv_mode', 'auto'));

  function parse(raw: string): Plane[] {
    try {
      const value: unknown = JSON.parse(raw === '' ? '[]' : raw);
      if (!Array.isArray(value)) return [];
      return value
        .filter(
          (item): item is Record<string, unknown> => typeof item === 'object' && item !== null,
        )
        .map((item) => ({
          name: typeof item.name === 'string' ? item.name : '',
          kwp: Number(item.kwp ?? 0),
          tilt: Number(item.tilt ?? 25),
          azimuth: Number(item.azimuth ?? 180),
        }));
    } catch {
      return [];
    }
  }

  function write(rows: Plane[]): void {
    draft.set(
      'pv_planes',
      JSON.stringify(rows.map(({ name, kwp, tilt, azimuth }) => ({ name, kwp, tilt, azimuth }))),
    );
  }

  /** The list, or the legacy single plane when the list was never written. */
  const stored = $derived(parse(draft.text('pv_planes')));
  const legacy = $derived.by((): Plane[] => {
    const kwp = Number(draft.text('pv_dc_kwp', '0'));
    if (!(kwp > 0)) return [];
    return [
      {
        name: 'Array',
        kwp,
        tilt: Number(draft.text('pv_tilt_deg', '25')),
        azimuth: Number(draft.text('pv_azimuth_deg', '135')),
      },
    ];
  });
  const planes = $derived(stored.length > 0 ? stored : legacy);
  const totalKwp = $derived(planes.reduce((sum, plane) => sum + plane.kwp, 0));

  function update(index: number, patch: Partial<Plane>): void {
    write(planes.map((plane, at) => (at === index ? { ...plane, ...patch } : plane)));
  }

  function add(): void {
    write([...planes, { name: `PV ${planes.length + 1}`, kwp: 0, tilt: 25, azimuth: 180 }]);
  }

  function remove(index: number): void {
    write(planes.filter((_, at) => at !== index));
  }

  function updateScalar(key: string, event: Event): void {
    const raw = (event.currentTarget as HTMLInputElement).value;
    const value = raw === '' ? null : Number(raw);
    draft.set(key, value === null || Number.isNaN(value) ? null : value);
  }
</script>

<div class="space-y-2.5">
  <div class="flex items-start gap-2">
    <Sun class="mt-0.5 size-3.5 text-pv" />
    <p class="text-[10px] leading-relaxed text-faint">
      One row per roof plane: the model serves each at its own azimuth and sums them, then applies
      the inverter ceiling once. Open-Meteo supplies the weather over Home Assistant's coordinates.
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

  <div class="space-y-2">
    <div class="flex items-baseline justify-between">
      <p class="text-[11px] text-muted">Array</p>
      <p class="text-[10px] text-faint">{totalKwp.toFixed(2)} kWp total</p>
    </div>
    {#each planes as plane, index (index)}
      <div class="rounded-md border border-line bg-panel/60 p-2">
        <div class="flex items-center gap-1.5">
          <input
            type="text"
            placeholder="PV SE"
            value={plane.name}
            oninput={(event) => update(index, { name: event.currentTarget.value })}
            class="min-w-0 flex-1 rounded border border-line bg-panel px-2 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
          />
          <button
            type="button"
            aria-label="Remove this plane"
            onclick={() => remove(index)}
            class="rounded border border-line p-1 text-faint hover:border-line-strong hover:text-ink"
          >
            <Trash2 class="size-3.5" />
          </button>
        </div>
        <div class="mt-2 grid grid-cols-3 gap-1.5">
          <label class="block">
            <span class="text-[10px] text-faint">kWp</span>
            <input
              type="number"
              step="0.01"
              value={plane.kwp}
              onchange={(event) => update(index, { kwp: Number(event.currentTarget.value) || 0 })}
              class="mt-0.5 w-full rounded border border-line bg-panel px-1.5 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
            />
          </label>
          <label class="block">
            <span class="text-[10px] text-faint">Tilt °</span>
            <input
              type="number"
              step="1"
              value={plane.tilt}
              onchange={(event) => update(index, { tilt: Number(event.currentTarget.value) || 0 })}
              class="mt-0.5 w-full rounded border border-line bg-panel px-1.5 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
            />
          </label>
          <label class="block">
            <span class="text-[10px] text-faint">Azimuth °</span>
            <input
              type="number"
              step="1"
              value={plane.azimuth}
              onchange={(event) =>
                update(index, { azimuth: Number(event.currentTarget.value) || 0 })}
              class="mt-0.5 w-full rounded border border-line bg-panel px-1.5 py-1 text-[11px] text-ink focus:border-line-strong focus:outline-none"
            />
          </label>
        </div>
      </div>
    {/each}
    <button
      type="button"
      onclick={add}
      class="flex items-center gap-1 rounded border border-line px-2 py-1 text-[11px] text-muted hover:border-line-strong hover:text-ink"
    >
      <Plus class="size-3.5" /> Add plane
    </button>
    <p class="text-[10px] text-faint">Azimuth: 0 north, 90 east, 180 south, 270 west.</p>
  </div>

  <label class="block">
    <span class="text-[11px] text-muted">Inverter AC</span>
    <span class="ml-1 text-[10px] text-faint">kW</span>
    <input
      type="number"
      step="0.05"
      value={draft.text('pv_ac_kwp')}
      onchange={(event) => updateScalar('pv_ac_kwp', event)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
    />
    <span class="mt-0.5 block text-[10px] text-faint">
      The single ceiling over all planes; clipping is applied to the total.
    </span>
  </label>

  <label class="block">
    <span class="text-[11px] text-muted">Model scale</span>
    <span class="ml-1 text-[10px] text-faint">×</span>
    <input
      type="number"
      step="0.01"
      value={draft.text('pv_scale')}
      onchange={(event) => updateScalar('pv_scale', event)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
    />
    <span class="mt-0.5 block text-[10px] text-faint">Calibrates the model to the array.</span>
  </label>
</div>
