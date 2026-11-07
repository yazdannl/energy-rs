<script lang="ts">
  import type { PriceSlot } from '../../lib/api/types';
  import { fmtEurPerKwh } from '../../lib/format/money';
  import { clock } from '../../lib/format/time';

  interface Props {
    slots: PriceSlot[];
  }

  let { slots }: Props = $props();

  const values = $derived(slots.map((slot) => slot.buyEurPerKwh));
  const min = $derived(values.length === 0 ? 0 : Math.min(...values));
  const max = $derived(values.length === 0 ? 0 : Math.max(...values));
  const span = $derived(max - min || 1);

  /** Cheap is good (charge), expensive is bad (discharge); intensity grows away from the middle. */
  function cellStyle(value: number): string {
    const t = (value - min) / span;
    const strength = Math.round(16 + Math.abs(t - 0.5) * 2 * 62);
    const tone = t <= 0.5 ? 'var(--ui-good)' : 'var(--ui-bad)';
    return `background: color-mix(in oklab, ${tone} ${strength}%, var(--ui-panel))`;
  }

  const days = $derived.by(() => {
    const grouped: { label: string; slots: PriceSlot[] }[] = [];
    for (const slot of slots) {
      const label = new Date(slot.startAt).toLocaleDateString(undefined, {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
      });
      const existing = grouped.find((group) => group.label === label);
      if (existing === undefined) grouped.push({ label, slots: [slot] });
      else existing.slots.push(slot);
    }
    return grouped;
  });
</script>

<div class="space-y-2">
  <div class="-mx-1 overflow-x-auto px-1 pb-1">
    <div class="flex w-full min-w-[620px] gap-4" aria-hidden="true">
      {#each days as { label, slots: daySlots } (label)}
        <div class="min-w-0 space-y-1" style="flex:{daySlots.length} 1 0">
          <p class="truncate text-[10px] font-medium uppercase tracking-[0.06em] text-faint">
            {label}
          </p>
          <div class="flex items-end gap-[1.5px]">
            {#each daySlots as slot (slot.startAt)}
              <div
                class="h-10 min-w-[2px] flex-1 rounded-[2px] transition-transform duration-150 hover:scale-y-110 hover:outline hover:outline-1 hover:outline-ink/40"
                style={cellStyle(slot.buyEurPerKwh)}
                title={`${clock(slot.startAt)} · ${fmtEurPerKwh(slot.buyEurPerKwh)}`}
              ></div>
            {/each}
          </div>
          <div class="flex justify-between text-[9px] text-faint">
            <span>00</span>
            <span>12</span>
            <span>24</span>
          </div>
        </div>
      {/each}
    </div>
  </div>

  <div class="flex items-center justify-between gap-3 text-[10px] text-faint">
    <span>Each bar is one quarter-hour, in local time.</span>
    <span class="flex items-center gap-1.5">
      <span class="num">{fmtEurPerKwh(min)}</span>
      <span
        class="h-1.5 w-20 rounded-full"
        style="background: linear-gradient(90deg, color-mix(in oklab, var(--ui-good) 78%, var(--ui-panel)), var(--ui-panel), color-mix(in oklab, var(--ui-bad) 78%, var(--ui-panel)))"
      ></span>
      <span class="num">{fmtEurPerKwh(max)}</span>
    </span>
  </div>
</div>
