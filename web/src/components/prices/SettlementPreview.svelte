<script lang="ts">
  import Scale from '@lucide/svelte/icons/scale';

  import type { PriceSlot } from '../../lib/api/types';
  import { fmtEur, fmtEurPerKwh, round6 } from '../../lib/format/money';
  import { clock, slotDay } from '../../lib/format/time';
  import {
    FEED_IN_FEE,
    exportPrice,
    netMeteringActive,
    salderingHint,
    TERUGLEVER_KOSTEN,
  } from '../../lib/format/settlement';
  import Badge from '../ui/Badge.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import Tooltip from '../ui/Tooltip.svelte';

  interface Props {
    slots: PriceSlot[];
  }

  let { slots }: Props = $props();

  const regime = $derived(netMeteringActive(new Date()));
  let previewNetting = $state(netMeteringActive(new Date()));

  const rows = $derived(
    slots.map((slot) => {
      const preview = exportPrice(slot, previewNetting);
      const api = slot.feedInEurPerKwh;
      return { slot, preview, api, delta: round6(preview - api) };
    }),
  );

  const average = $derived(
    rows.length === 0
      ? 0
      : round6(rows.reduce((total, row) => total + row.preview, 0) / rows.length),
  );
  const zeroCount = $derived(rows.filter((row) => row.preview === 0).length);
  const best = $derived(
    rows.reduce<{ slot: PriceSlot; preview: number } | null>(
      (top, row) => (top === null || row.preview > top.preview ? row : top),
      null,
    ),
  );
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="flex items-center gap-2">
      <Segmented
        label="Preview regime"
        value={previewNetting ? 'on' : 'off'}
        options={[
          { value: 'on', label: 'Netting on', hint: 'Export earns the all-in import price' },
          { value: 'off', label: 'Netting off', hint: 'Export earns the post-saldering feed-in' },
        ]}
        onselect={(value) => (previewNetting = value === 'on')}
      />
      <Badge tone={regime ? 'good' : 'warn'}>{regime ? 'regime: netting' : 'regime: feed-in'}</Badge
      >
    </div>
    <Tooltip label="How the preview is computed">
      Mirrors <span class="font-mono">Settlement::export_price</span>: with netting the export price
      is
      <span class="num">max(buy, 0)</span>; without it,
      <span class="num">(market + €{FEED_IN_FEE.toFixed(2)}) − €{TERUGLEVER_KOSTEN.toFixed(2)}</span
      >, clamped at 0. The API's <span class="font-mono">feedInEurPerKwh</span> stays authoritative.
    </Tooltip>
  </div>

  <p class="text-[10.5px] leading-relaxed text-muted">{salderingHint(new Date())}</p>

  <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
    <div class="rounded-lg border border-line bg-panel-soft/60 px-3 py-2">
      <p class="text-[10px] uppercase tracking-[0.07em] text-faint">Avg export price</p>
      <p class="num text-[15px] font-semibold text-ink">{fmtEurPerKwh(average)}</p>
    </div>
    <div class="rounded-lg border border-line bg-panel-soft/60 px-3 py-2">
      <p class="text-[10px] uppercase tracking-[0.07em] text-faint">Clamped at zero</p>
      <p class="num text-[15px] font-semibold text-ink">
        {zeroCount}<span class="text-[11px] text-faint"> / {rows.length}</span>
      </p>
    </div>
    <div class="rounded-lg border border-line bg-panel-soft/60 px-3 py-2">
      <p class="text-[10px] uppercase tracking-[0.07em] text-faint">Best export slot</p>
      <p class="num text-[15px] font-semibold text-good">
        {best === null ? '—' : fmtEurPerKwh(best.preview)}
      </p>
      {#if best !== null}
        <p class="text-[10px] text-faint">
          {slotDay(best.slot.startAt)}
          {clock(best.slot.startAt)}
        </p>
      {/if}
    </div>
  </div>

  <p class="text-[10px] text-faint">
    Δ is the preview against the API's published feed-in; switch the regime to watch it move.
  </p>

  <div class="max-h-[320px] overflow-y-auto rounded-lg border border-line">
    <table class="w-full border-collapse text-[11px]">
      <thead class="sticky top-0 z-10 bg-panel">
        <tr class="text-left text-[10px] uppercase tracking-[0.06em] text-faint">
          <th class="px-2 py-1.5 font-medium">Slot</th>
          <th class="px-2 py-1.5 text-right font-medium">Market</th>
          <th class="px-2 py-1.5 text-right font-medium">Buy</th>
          <th class="px-2 py-1.5 text-right font-medium">API feed-in</th>
          <th class="px-2 py-1.5 text-right font-medium">Preview</th>
          <th class="px-2 py-1.5 text-right font-medium">Δ</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row (row.slot.startAt)}
          <tr class="border-t border-line/60">
            <td class="num px-2 py-1 text-muted"
              >{slotDay(row.slot.startAt)} {clock(row.slot.startAt)}</td
            >
            <td class="num px-2 py-1 text-right text-muted"
              >{fmtEurPerKwh(row.slot.marketEurPerKwh)}</td
            >
            <td class="num px-2 py-1 text-right text-ink">{fmtEurPerKwh(row.slot.buyEurPerKwh)}</td>
            <td class="num px-2 py-1 text-right text-muted">{fmtEurPerKwh(row.api)}</td>
            <td class="num px-2 py-1 text-right font-medium text-good"
              >{fmtEurPerKwh(row.preview)}</td
            >
            <td
              class={[
                'num px-2 py-1 text-right',
                row.delta === 0 ? 'text-faint' : row.delta > 0 ? 'text-good' : 'text-bad',
              ]}
            >
              {fmtEur(row.delta)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<p class="flex items-center gap-1.5 text-[10px] text-faint">
  <Scale class="size-3" aria-hidden="true" />
  What-if only: nothing here is written to the ledger and the engine keeps using the API's price.
</p>
