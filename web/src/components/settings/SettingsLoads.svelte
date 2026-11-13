<script lang="ts">
  import Home from '@lucide/svelte/icons/house';
  import Plus from '@lucide/svelte/icons/plus';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { onMount } from 'svelte';

  import type { LoadDevice, LoadsInput, LoadsResponse } from '../../lib/api/types';
  import { ApiError } from '../../lib/api/errors';
  import { api } from '../../lib/state/runtime';
  import { toasts } from '../../lib/state/toast.svelte';
  import Button from '../ui/Button.svelte';
  import ErrorNote from '../ui/ErrorNote.svelte';
  import LoadSources from './LoadSources.svelte';
  import Skeleton from '../ui/Skeleton.svelte';
  import LoadDeviceEditor from './LoadDeviceEditor.svelte';

  let loads = $state<LoadsResponse | null>(null);
  let draft = $state<LoadsInput>({
    holiday: false,
    temperatureEntity: '',
    weatherEntity: '',
    loadEntity: '',
    devices: [],
  });
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<ApiError | null>(null);

  const dirty = $derived(
    loads !== null &&
      JSON.stringify({
        holiday: loads.holiday,
        temperatureEntity: loads.temperatureEntity,
        weatherEntity: loads.weatherEntity,
        loadEntity: loads.loadEntity,
        devices: loads.devices,
      }) !== JSON.stringify(draft),
  );

  function adopt(response: LoadsResponse): void {
    loads = response;
    draft = {
      holiday: response.holiday,
      temperatureEntity: response.temperatureEntity,
      weatherEntity: response.weatherEntity,
      loadEntity: response.loadEntity,
      devices: response.devices.map((device) => ({ ...device })),
    };
  }

  async function load(): Promise<void> {
    loading = true;
    try {
      adopt(await api.loads());
      error = null;
    } catch (cause) {
      error = cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
    } finally {
      loading = false;
    }
  }

  async function save(): Promise<void> {
    saving = true;
    try {
      adopt(await api.saveLoads(draft));
      toasts.push(
        'ok',
        'Load settings saved',
        'The profile is relearning now; a named load is modelled once its meter has been split off the house history.',
      );
    } catch (cause) {
      const refusal =
        cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
      toasts.push(
        'error',
        'The load settings were refused',
        `${refusal.message} (${refusal.code})`,
      );
    } finally {
      saving = false;
    }
  }

  function addDevice(): void {
    const device: LoadDevice = {
      entity: '',
      kind: 'heat_pump',
      name: '',
      keepWhileAway: false,
    };
    draft.devices = [...draft.devices, device];
  }

  function removeDevice(index: number): void {
    draft.devices = draft.devices.filter((_, at) => at !== index);
  }

  function updateDevice(index: number, patch: Partial<LoadDevice>): void {
    draft.devices = draft.devices.map((device, at) =>
      at === index ? { ...device, ...patch } : device,
    );
  }

  const modelled = $derived(
    new Map((loads?.modelled ?? []).map((row) => [row.entity, row.modelled])),
  );

  onMount(() => {
    void load();
  });
</script>

{#if loading && loads === null}
  <Skeleton rows={5} />
{:else if error !== null && loads === null}
  <ErrorNote {error} onretry={() => void load()} />
{:else}
  <div class="space-y-2.5">
    <div class="flex items-center justify-between gap-3">
      <div class="flex items-start gap-2">
        <Home class="mt-0.5 size-3.5 text-grid" />
        <div>
          <p class="text-[11px] text-muted">Holiday mode</p>
          <p class="text-[10px] text-faint">
            Serve the base load plus only the loads marked “runs while away”. Learning pauses, so an
            empty house never becomes the profile.
          </p>
        </div>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={draft.holiday}
        aria-label="Holiday mode"
        onclick={() => (draft.holiday = !draft.holiday)}
        class="relative h-5 w-9 shrink-0 rounded-full border transition-colors {draft.holiday
          ? 'border-good bg-good/30'
          : 'border-line bg-panel'}"
      >
        <span
          class="absolute top-0.5 size-3.5 rounded-full transition-all {draft.holiday
            ? 'left-4.5 bg-good'
            : 'left-0.5 bg-line-strong'}"
        ></span>
      </button>
    </div>

    <LoadSources
      bind:loadEntity={draft.loadEntity}
      bind:temperatureEntity={draft.temperatureEntity}
      bind:weatherEntity={draft.weatherEntity}
    />

    <div class="space-y-2">
      <p class="text-[11px] text-muted">Named loads</p>
      {#each draft.devices as device, index (index)}
        <LoadDeviceEditor
          {device}
          modelled={modelled.get(device.entity.trim())}
          onupdate={(patch) => updateDevice(index, patch)}
          onremove={() => removeDevice(index)}
        />
      {/each}
      <div class="flex items-center justify-between gap-2">
        <Button onclick={addDevice} title="Add a meter">
          {#snippet icon()}<Plus class="size-3.5" />{/snippet}
          Add load
        </Button>
        <Button
          variant="primary"
          busy={saving}
          disabled={!dirty}
          onclick={() => void save()}
          title="PATCH /loads with the whole configuration"
        >
          {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
          {dirty ? 'Save loads' : 'Saved'}
        </Button>
      </div>
    </div>

    <div
      class="rounded-md border border-line bg-panel/40 p-2 text-[10px] leading-relaxed text-faint"
    >
      {#if loads?.profile}
        <p>
          Learned base load <span class="font-mono text-ink"
            >{(loads.profile.baseKw * 1000).toFixed(0)} W</span
          >
          · house floor
          <span class="font-mono text-ink">{(loads.profile.houseBaseKw * 1000).toFixed(0)} W</span>
          · {loads.profile.days} days, {loads.profile.samples} hours, {loads.profile
            .temperatureHours} with a temperature reading. No hour is predicted below the house floor.
        </p>
      {:else}
        <p>No usable learned profile yet; the residential shape is serving the plan.</p>
      {/if}
      {#if (loads?.profile?.accounting.clampedHours ?? 0) > 0}
        <p>
          {loads?.profile?.accounting.clampedHours} house hours were left out of the base profile: the
          meters claimed more than the house total there, so they overlap (the hour is unknown, not zero).
        </p>
      {/if}
      {#if (loads?.profile?.accounting.droppedHours ?? 0) > 0}
        <p>
          {loads?.profile?.accounting.droppedHours} house hours were left out because a meter had no reading
          for them.
        </p>
      {/if}
      {#each loads?.profile?.accounting.excluded ?? [] as entry (entry.key)}
        <p class="text-warn">· {entry.key}: {entry.reason}</p>
      {/each}
      {#each loads?.problems ?? [] as problem (problem)}
        <p class="text-warn">· {problem}</p>
      {/each}
    </div>
  </div>
{/if}
