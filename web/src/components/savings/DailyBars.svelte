<script lang="ts">
  import type { SavingsDay } from '../../lib/api/types';
  import { fmtEur } from '../../lib/format/money';
  import { dayShort } from '../../lib/format/time';
  import EmptyNote from '../ui/EmptyNote.svelte';

  interface Props {
    days: SavingsDay[];
  }

  let { days }: Props = $props();

  const peak = $derived(Math.max(0.01, ...days.map((day) => Math.abs(day.savingEur))));
  const height = 42;
  const width = 100;
  const width1 = $derived(days.length === 0 ? width : width / days.length);

  function barHeight(day: SavingsDay): number {
    return (Math.abs(day.savingEur) / peak) * (height - 6);
  }
</script>

{#if days.length === 0}
  <EmptyNote
    title="No daily rows"
    detail="The recorder has not settled a day for this period yet."
  />
{:else}
  <div class="space-y-1.5">
    <svg
      viewBox="0 0 {width} {height}"
      preserveAspectRatio="none"
      class="h-32 w-full"
      role="img"
      aria-label={`Daily saving, ${days.length} days, peak ${fmtEur(peak)}`}
    >
      <line
        x1="0"
        y1={height - 2}
        x2={width}
        y2={height - 2}
        stroke="var(--ui-line-strong)"
        stroke-width="0.3"
      />
      {#each days as day, index (day.day)}
        {@const bar = barHeight(day)}
        <rect
          x={index * width1 + width1 * 0.18}
          y={height - 3 - bar}
          width={width1 * 0.64}
          height={bar}
          rx="0.6"
          fill={day.savingEur >= 0 ? 'var(--ui-good)' : 'var(--ui-bad)'}
          opacity={day.complete ? 0.85 : 0.4}
        >
          <title
            >{day.day} · {fmtEur(day.savingEur)} · {day.complete
              ? 'complete'
              : `${day.coverageHours.toFixed(1)} h covered`}</title
          >
        </rect>
      {/each}
    </svg>
    <div class="flex justify-between text-[9.5px] text-faint">
      <span>{dayShort(days[0].day)}</span>
      <span>{dayShort(days[Math.floor(days.length / 2)].day)}</span>
      <span>{dayShort(days.at(-1)?.day ?? days[0].day)}</span>
    </div>
    <p class="text-[10px] text-faint">
      Peak {fmtEur(peak)} · faded bars are partial days (their coverage is shown, never dropped).
    </p>
  </div>
{/if}
