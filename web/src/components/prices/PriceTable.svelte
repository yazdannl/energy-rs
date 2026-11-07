<script lang="ts">
  import type { PriceSlot } from '../../lib/api/types';
  import { fmtEurPerKwh } from '../../lib/format/money';
  import { clock } from '../../lib/format/time';

  interface Props {
    slots: PriceSlot[];
  }

  let { slots }: Props = $props();

  /** One row per hour: the four quarter-hours collapsed to a range, because that is what bills. */
  const hours = $derived.by(() => {
    const rows: { key: string; label: string; low: number; high: number; feedIn: number }[] = [];
    for (const slot of slots) {
      const date = new Date(slot.startAt);
      const key = `${date.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' })} ${date.getHours()}`;
      const row = rows.at(-1);
      if (row !== undefined && row.key === key) {
        row.low = Math.min(row.low, slot.buyEurPerKwh);
        row.high = Math.max(row.high, slot.buyEurPerKwh);
        row.feedIn = slot.feedInEurPerKwh;
      } else {
        rows.push({
          key,
          label: clock(slot.startAt),
          low: slot.buyEurPerKwh,
          high: slot.buyEurPerKwh,
          feedIn: slot.feedInEurPerKwh,
        });
      }
    }
    return rows;
  });
</script>

<div class="max-h-[300px] overflow-y-auto pr-1">
  <table class="w-full border-collapse text-[11px]">
    <thead class="sticky top-0 z-10 bg-panel">
      <tr class="text-left text-[10px] uppercase tracking-[0.06em] text-faint">
        <th class="py-1.5 font-medium">From</th>
        <th class="py-1.5 text-right font-medium">Buy low</th>
        <th class="py-1.5 text-right font-medium">Buy high</th>
        <th class="py-1.5 text-right font-medium">Feed-in</th>
      </tr>
    </thead>
    <tbody>
      {#each hours as hour (hour.key)}
        <tr class="border-t border-line/70">
          <td class="num py-1.5 text-muted">{hour.label}</td>
          <td class="num py-1.5 text-right text-good">{fmtEurPerKwh(hour.low)}</td>
          <td class="num py-1.5 text-right text-bad">{fmtEurPerKwh(hour.high)}</td>
          <td class="num py-1.5 text-right text-ink">{fmtEurPerKwh(hour.feedIn)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
