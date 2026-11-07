<script lang="ts">
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import PiggyBank from '@lucide/svelte/icons/piggy-bank';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { onMount } from 'svelte';

  import { fmtEur, round6 } from '../lib/format/money';
  import { dayLong } from '../lib/format/time';
  import { api } from '../lib/state/runtime';
  import { poll } from '../lib/state/poller';
  import { resource } from '../lib/state/resource.svelte';
  import PageHeader from '../components/layout/PageHeader.svelte';
  import DailyBars from '../components/savings/DailyBars.svelte';
  import SavingsBreakdown from '../components/savings/SavingsBreakdown.svelte';
  import SavingsCard from '../components/savings/SavingsCard.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import Button from '../components/ui/Button.svelte';
  import EmptyNote from '../components/ui/EmptyNote.svelte';
  import ErrorNote from '../components/ui/ErrorNote.svelte';
  import Panel from '../components/ui/Panel.svelte';
  import Skeleton from '../components/ui/Skeleton.svelte';

  const day = resource(() => api.savings('day'));
  const month = resource(() => api.savings('month'));
  const lifetime = resource(() => api.savings('lifetime'));

  function reload(): void {
    void day.load();
    void month.load();
    void lifetime.load();
  }

  onMount(() => {
    reload();
    const stopPoll = poll(reload, 900_000);
    return () => {
      stopPoll();
      day.dispose();
      month.dispose();
      lifetime.dispose();
    };
  });

  /** 7 d is not an API period: it is the last seven rows of the month reply, and says so. */
  const week = $derived.by(() => {
    const rows = month.data?.days.slice(-7) ?? [];
    return {
      saving: rows.length === 0 ? null : round6(rows.reduce((sum, row) => sum + row.savingEur, 0)),
      baseline:
        rows.length === 0 ? null : round6(rows.reduce((sum, row) => sum + row.baselineEur, 0)),
      actual: rows.length === 0 ? null : round6(rows.reduce((sum, row) => sum + row.actualEur, 0)),
      days: rows.length,
      completeDays: rows.filter((row) => row.complete).length,
      partial: rows.length > 0 && rows.length < 7,
    };
  });

  const loading = $derived(day.loading || month.loading || lifetime.loading);
  const failure = $derived(day.error ?? month.error ?? lifetime.error);
</script>

<PageHeader
  title="Savings"
  summary="Measured days from the ledger — planned values never appear here"
>
  {#snippet actions()}
    {#if lifetime.data}
      <Badge tone="neutral">{lifetime.data.totals.days} recorded days</Badge>
    {/if}
    <Button onclick={reload} title="Reload savings rollups">
      {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
      Reload
    </Button>
  {/snippet}
</PageHeader>

{#if failure}
  <div class="mt-4"><ErrorNote error={failure} onretry={reload} /></div>
{:else if loading && day.data === null && month.data === null}
  <div class="mt-4 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
    {#each [0, 1, 2, 3] as cell (cell)}
      <div class="card p-3.5"><Skeleton rows={3} /></div>
    {/each}
  </div>
{:else}
  <div class="mt-4 grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
    <SavingsCard
      label="Today"
      saving={day.data?.totals.savingEur ?? null}
      baseline={day.data?.totals.baselineEur ?? null}
      actual={day.data?.totals.actualEur ?? null}
      days={day.data?.totals.days ?? 0}
      completeDays={day.data?.totals.completeDays ?? 0}
      note={day.data?.days.at(-1)?.complete === false
        ? `partial day · ${day.data?.days.at(-1)?.coverageHours.toFixed(1)} h covered`
        : undefined}
    />
    <SavingsCard
      label="Last 7 days"
      saving={week.saving}
      baseline={week.baseline}
      actual={week.actual}
      days={week.days}
      completeDays={week.completeDays}
      note={week.partial ? 'only part of the window has rows' : 'summed from month rows'}
    />
    <SavingsCard
      label="This month"
      saving={month.data?.totals.savingEur ?? null}
      baseline={month.data?.totals.baselineEur ?? null}
      actual={month.data?.totals.actualEur ?? null}
      days={month.data?.totals.days ?? 0}
      completeDays={month.data?.totals.completeDays ?? 0}
      note={month.data
        ? `${dayLong(month.data.fromDay)} → ${dayLong(month.data.toDay)}`
        : undefined}
    />
    <SavingsCard
      label="Lifetime"
      saving={lifetime.data?.totals.savingEur ?? null}
      baseline={lifetime.data?.totals.baselineEur ?? null}
      actual={lifetime.data?.totals.actualEur ?? null}
      days={lifetime.data?.totals.days ?? 0}
      completeDays={lifetime.data?.totals.completeDays ?? 0}
      note={lifetime.data
        ? `${dayLong(lifetime.data.fromDay)} → ${dayLong(lifetime.data.toDay)}`
        : undefined}
    />
  </div>

  <div class="mt-3 grid gap-3 xl:grid-cols-2">
    <Panel title="Daily saving" subtitle="This month, one bar per settled day">
      {#snippet icon()}<CalendarDays class="size-4 text-good" />{/snippet}
      {#if month.data}
        <DailyBars days={month.data.days} />
      {:else}
        <EmptyNote title="No month rows" />
      {/if}
    </Panel>

    <Panel title="Breakdown" subtitle="Measured energy per category">
      {#snippet icon()}<PiggyBank class="size-4 text-pv" />{/snippet}
      {#if month.data && month.data.totals.days > 0}
        <SavingsBreakdown totals={month.data.totals} />
      {:else}
        <EmptyNote
          title="No data for this month"
          detail="No settled day yet, so there is nothing to split."
        />
      {/if}
      <p class="mt-2 text-[10px] text-faint">
        Month saving so far: {fmtEur(month.data?.totals.savingEur)} over {month.data?.totals.days ??
          0}
        day{(month.data?.totals.days ?? 0) === 1 ? '' : 's'}.
      </p>
    </Panel>
  </div>
{/if}
