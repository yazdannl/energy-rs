<script lang="ts">
  import ChartLine from '@lucide/svelte/icons/chart-line';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { onMount, untrack } from 'svelte';

  import { DEFAULT_LAYERS, type PlanLayers } from '../lib/chart/planChart';
  import { ago, span } from '../lib/format/time';
  import { api } from '../lib/state/runtime';
  import { connection } from '../lib/state/connection.svelte';
  import { poll } from '../lib/state/poller';
  import { resource } from '../lib/state/resource.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import LayerToggles from '../components/plan/LayerToggles.svelte';
  import PlanChart from '../components/plan/PlanChart.svelte';
  import PlanSummary from '../components/plan/PlanSummary.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import EmptyNote from '../components/ui/EmptyNote.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';

  const plan = resource(() => api.currentPlan());
  let layers = $state<PlanLayers>({ ...DEFAULT_LAYERS });
  let replanning = $state(false);
  let replanNote = $state<string | null>(null);

  /** Ask the engine for a fresh solve; the plan arrives with the next snapshot. */
  async function replan(): Promise<void> {
    replanning = true;
    replanNote = null;
    try {
      await api.replan();
      replanNote = 'requested — the next tick solves it';
    } catch (cause) {
      replanNote = cause instanceof Error ? cause.message : String(cause);
    } finally {
      replanning = false;
    }
  }

  onMount(() => {
    void plan.load();
    const stopPoll = poll(() => void plan.load(), 900_000);
    return () => {
      stopPoll();
      plan.dispose();
    };
  });

  $effect(() => {
    const version = connection.planVersion;
    if (version === 0) return;
    untrack(() => void plan.load());
  });
</script>

<PageHeader title="Plan" summary="30 h × 15 min from the optimizer, with the reasons that bind it">
  {#snippet actions()}
    {#if plan.data}
      <Badge tone="neutral">solved {ago(plan.data.createdAt)}</Badge>
    {/if}
    <Button
      onclick={() => void replan()}
      disabled={replanning}
      title="Ask the optimizer for a fresh plan now"
    >
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      {replanning ? 'Requesting…' : 'Replan'}
    </Button>
    <Button onclick={() => void plan.load()} title="Reload the current plan">
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      Reload
    </Button>
  {/snippet}
</PageHeader>

{#if replanNote}
  <p class="mt-2 text-[10.5px] text-faint">{replanNote}</p>
{/if}

{#if plan.loading && plan.data === null}
  <div class="mt-4 space-y-3">
    <div class="card p-4"><Skeleton rows={5} /></div>
    <div class="card p-4"><Skeleton rows={2} /></div>
  </div>
{:else if plan.error}
  <div class="mt-4"><ErrorNote error={plan.error} onretry={() => void plan.load()} /></div>
{:else if plan.data === null}
  <div class="mt-4">
    <EmptyNote
      title="No plan yet"
      detail="The engine stores a plan after its first solve. Nothing is fabricated in the meantime: the API answers 404 and this screen says so."
    />
  </div>
{:else}
  <div class="mt-4 space-y-3">
    <div class="card p-4">
      <PlanSummary plan={plan.data} />
    </div>

    <Panel
      title="Timeline"
      subtitle={`${span(plan.data.horizonStartAt, plan.data.horizonEndAt)} · ${plan.data.slots.length} slots`}
    >
      {#snippet icon()}<ChartLine class="size-4 text-batt" />{/snippet}
      {#snippet actions()}
        <LayerToggles {layers} onchange={(next) => (layers = next)} />
      {/snippet}
      <PlanChart plan={plan.data} {layers} />
    </Panel>

    {#if plan.data.summary.terminalTargetWh !== null}
      <p class="text-[10.5px] text-faint">
        Terminal target: {plan.data.summary.terminalTargetWh} Wh at the end of the horizon.
      </p>
    {/if}
  </div>
{/if}
