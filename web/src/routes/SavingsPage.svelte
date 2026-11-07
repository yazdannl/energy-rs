<script lang="ts">
  import CalendarDays from '@lucide/svelte/icons/calendar-days';
  import PiggyBank from '@lucide/svelte/icons/piggy-bank';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import { onMount } from 'svelte';

  import type { SavingsDay } from '../lib/api/types';
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
    const rows = (month.data?.days ?? []).slice(-7);
    const measured = rows.filter(
      (row): row is SavingsDay & { savingEur: number; baselineEur: number; actualEur: number } =>
        row.savingEur !== null && row.baselineEur !== null && row.actualEur !== null,
    );
    // The cards say "no data" when nothing was counted, not "€0.00".
    return {
      saving:
        measured.length === 0
          ? null
          : round6(measured.reduce((sum, row) => sum + row.savingEur, 0)),
      baseline:
        measured.length === 0
          ? null
          : round6(measured.reduce((sum, row) => sum + row.baselineEur, 0)),
      actual:
        measured.length === 0
          ? null
          : round6(measured.reduce((sum, row) => sum + row.actualEur, 0)),
      days: rows.length,
      completeDays: measured.filter((row) => row.complete).length,
      missing: rows.length - measured.length,
      partial: measured.length < 7,
    };
  });

  /** Closed days with a gap keep their energy but carry no money; they are in no total. */
  const monthMissing = $derived(
    (month.data?.days ?? []).filter((day) => day.savingEur === null).length,
  );
  const monthCounted = $derived(
    (month.data?.days ?? []).filter((day) => day.savingEur !== null).length,
  );
  const today = $derived(day.data?.days.at(-1) ?? null);
  const todayCounted = $derived(today?.savingEur === null || today === null ? 0 : 1);

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
      countedDays={todayCounted}
      note={today?.savingEur === null
        ? `no money yet · ${today?.coverageHours.toFixed(1) ?? '0'} h covered`
        : today?.complete === false
          ? `partial day · ${today.coverageHours.toFixed(1)} h covered`
          : undefined}
    />
    <SavingsCard
      label="Last 7 days"
      saving={week.saving}
      baseline={week.baseline}
      actual={week.actual}
      days={week.days}
      completeDays={week.completeDays}
      countedDays={week.days - week.missing}
      note={week.missing > 0
        ? `${week.missing} day${week.missing === 1 ? '' : 's'} without money excluded`
        : week.partial
          ? 'only part of the window has rows'
          : 'summed from month rows'}
    />
    <SavingsCard
      label="This month"
      saving={month.data?.totals.savingEur ?? null}
      baseline={month.data?.totals.baselineEur ?? null}
      actual={month.data?.totals.actualEur ?? null}
      days={month.data?.totals.days ?? 0}
      completeDays={month.data?.totals.completeDays ?? 0}
      countedDays={monthCounted}
      note={month.data
        ? `${dayLong(month.data.fromDay)} → ${dayLong(month.data.toDay)}${monthMissing > 0 ? ` · ${monthMissing} with no money` : ''}`
        : undefined}
    />
    <SavingsCard
      label="Lifetime"
      saving={lifetime.data?.totals.savingEur ?? null}
      baseline={lifetime.data?.totals.baselineEur ?? null}
      actual={lifetime.data?.totals.actualEur ?? null}
      days={lifetime.data?.totals.days ?? 0}
      completeDays={lifetime.data?.totals.completeDays ?? 0}
      countedDays={lifetime.data?.totals.completeDays ?? 0}
      note={lifetime.data
        ? `${dayLong(lifetime.data.fromDay)} → ${dayLong(lifetime.data.toDay)} · complete days only`
        : undefined}
    />
  </div>

  {#if monthMissing > 0}
    <p class="mt-2 text-[10.5px] leading-relaxed text-muted">
      {monthMissing} closed day{monthMissing === 1 ? '' : 's'} in this month had a measurement gap: the
      day keeps its energy and coverage hours but carries <span class="font-mono">null</span> money, so
      it is in no total. The cards above say so with their day counts.
    </p>
  {/if}

  <div class="mt-3 grid items-start gap-3 xl:grid-cols-2">
    <Panel title="Daily saving" subtitle="This month, one bar per settled day">
      {#snippet icon()}<CalendarDays class="size-4 text-good" />{/snippet}
      {#if month.data}
        <DailyBars days={month.data.days} />
      {:else}
        <EmptyNote title="No month rows" />
      {/if}
    </Panel>

    <Panel title="Breakdown" subtitle="The four money components and what they are made of">
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
