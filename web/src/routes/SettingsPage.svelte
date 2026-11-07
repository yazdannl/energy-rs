<script lang="ts">
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';
  import Database from '@lucide/svelte/icons/database';
  import KeyRound from '@lucide/svelte/icons/key-round';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Scale from '@lucide/svelte/icons/scale';
  import Server from '@lucide/svelte/icons/server';
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import { onMount } from 'svelte';

  import type { DispatchResponse, Health, PricesResponse, SitePack } from '../lib/api/types';
  import { ApiError } from '../lib/api/errors';
  import { clockSeconds } from '../lib/format/time';
  import { api } from '../lib/state/runtime';
  import { connection } from '../lib/state/connection.svelte';
  import { SettingsDraft } from '../lib/state/settingsDraft.svelte';
  import { toasts } from '../lib/state/toast.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import SettingsBatteryList from '../components/settings/SettingsBatteryList.svelte';
  import SettingsContract from '../components/settings/SettingsContract.svelte';
  import SettingsHealth from '../components/settings/SettingsHealth.svelte';
  import SettingsLimits from '../components/settings/SettingsLimits.svelte';
  import SettingsProvider from '../components/settings/SettingsProvider.svelte';
  import SettingsRetention from '../components/settings/SettingsRetention.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import ModeSwitch from '../components/ui/ModeSwitch.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';

  const draft = new SettingsDraft();
  let health = $state<Health | null>(null);
  let healthError = $state<ApiError | null>(null);
  const site = $state<{ packs: SitePack[]; configured: boolean }>({ packs: [], configured: false });
  let dispatch = $state<DispatchResponse | null>(null);
  let prices = $state<PricesResponse | null>(null);
  let siteLoading = $state(true);

  async function loadHealth(): Promise<void> {
    try {
      health = await api.health();
      healthError = null;
    } catch (cause) {
      healthError = cause instanceof ApiError ? cause : new ApiError(0, 'internal', String(cause));
    }
  }

  async function loadSite(): Promise<void> {
    siteLoading = true;
    try {
      const response = await api.site();
      site.packs = response.packs;
      site.configured = response.configured;
      dispatch = await api.dispatch(5);
      prices = await api.prices();
    } catch {
      // The panels each show their own honest empty state; the save path does not depend on this.
    } finally {
      siteLoading = false;
    }
  }

  onMount(() => {
    void draft.load(() => api.settings());
    void loadHealth();
    void loadSite();
  });

  async function save(): Promise<void> {
    const error = await draft.save((values) => api.saveSettings(values));
    if (error === null) {
      toasts.push(
        'ok',
        'Settings saved',
        'Only the changed keys were written, through PATCH /settings.',
      );
      void loadSite();
    } else {
      toasts.push('error', 'The write was refused', `${error.message} (${error.code})`);
    }
  }
</script>

<PageHeader
  title="Settings"
  summary="Mode, contract, provider, limits and what the API actually exposes"
>
  {#snippet actions()}
    {#if draft.asOf}<Badge tone="neutral">as of {clockSeconds(draft.asOf)}</Badge>{/if}
    <Button
      variant="primary"
      busy={draft.saving}
      disabled={draft.dirty.length === 0}
      onclick={() => void save()}
      title="PATCH /settings with the changed keys only"
    >
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      {draft.dirty.length === 0 ? 'Saved' : `Save ${draft.dirty.length}`}
    </Button>
  {/snippet}
</PageHeader>

{#if draft.error && draft.schema.length === 0}
  <div class="mt-4">
    <ErrorNote error={draft.error} onretry={() => void draft.load(() => api.settings())} />
  </div>
{:else}
  <div class="mt-4 grid gap-3 xl:grid-cols-2">
    <Panel title="Control mode" subtitle="Persisted to the ledger before the engine is told">
      <ModeSwitch mode={connection.snapshot?.mode ?? health?.mode ?? null} />
      <p class="mt-2 text-[10px] leading-relaxed text-faint">
        A failed engine hand-off still saves the setting; the API answers 503 with
        <span class="font-mono">persisted: true</span> and the badge above keeps showing the new mode.
      </p>
    </Panel>

    <Panel title="API health" subtitle="Watchdog contract, not a solve indicator">
      {#snippet icon()}<Server class="size-4 text-good" />{/snippet}
      <SettingsHealth {health} error={healthError} onretry={() => void loadHealth()} />
    </Panel>

    <Panel title="Limits" subtitle="Whitelisted through PATCH /settings">
      {#snippet icon()}<SlidersHorizontal class="size-4 text-batt" />{/snippet}
      {#if draft.loading && draft.schema.length === 0}
        <Skeleton rows={5} />
      {:else}
        <SettingsLimits {draft} />
      {/if}
    </Panel>

    <Panel
      title="Price provider"
      subtitle="Provider and entity; the token stays in the add-on options"
    >
      {#snippet icon()}<KeyRound class="size-4 text-pv" />{/snippet}
      {#if draft.loading && draft.schema.length === 0}
        <Skeleton rows={4} />
      {:else}
        <SettingsProvider {draft} />
      {/if}
    </Panel>

    <Panel title="Contract" subtitle="Compiled constants the engine settles with">
      {#snippet icon()}<Scale class="size-4 text-grid" />{/snippet}
      <SettingsContract />
    </Panel>

    <Panel title="Batteries" subtitle="Discovered packs and their identity">
      {#snippet icon()}<BatteryCharging class="size-4 text-batt" />{/snippet}
      <SettingsBatteryList packs={site.packs} configured={site.configured} loading={siteLoading} />
    </Panel>

    <Panel title="Retention" subtitle="What the API keeps and for how long">
      {#snippet icon()}<Database class="size-4 text-muted" />{/snippet}
      <SettingsRetention {dispatch} {prices} />
    </Panel>

    <Panel title="Writes" subtitle="How a settings change reaches the ledger">
      <ul class="space-y-1.5 text-[10.5px] leading-relaxed text-muted">
        <li>• Only changed keys are sent; unchanged ones are not rewritten.</li>
        <li>
          • A write older than the stored change is refused with <span class="font-mono"
            >409 stale_write</span
          > — history is never rewritten.
        </li>
        <li>
          • An out-of-range value is a <span class="font-mono">422</span>; the form keeps your draft
          so nothing is lost.
        </li>
        <li>• The mode has its own endpoint, so it is not part of this PATCH.</li>
      </ul>
    </Panel>
  </div>

  {#if draft.dirty.length > 0}
    <div
      class="sticky bottom-16 z-20 mt-3 flex items-center justify-between gap-2 rounded-xl border border-line-strong bg-panel/95 px-3 py-2 backdrop-blur md:bottom-4"
    >
      <p class="text-[11px] text-muted">
        {draft.dirty.length} unsaved change{draft.dirty.length === 1 ? '' : 's'}:
        <span class="font-mono text-[10px] text-ink">{draft.dirty.join(', ')}</span>
      </p>
      <div class="flex items-center gap-1.5">
        <Button onclick={() => draft.reset()} title="Discard the draft">Discard</Button>
        <Button variant="primary" busy={draft.saving} onclick={() => void save()}
          >Save changes</Button
        >
      </div>
    </div>
  {/if}
{/if}
