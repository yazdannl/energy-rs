<script lang="ts">
  import Scale from '@lucide/svelte/icons/scale';

  import {
    BONUS_CAP_KWH,
    BONUS_PCT,
    ENERGY_TAX_2026_EX,
    FEED_IN_FEE,
    FEE_INCL,
    SALDERING_END_YEAR,
    TERUGLEVER_KOSTEN,
    VAT_NL,
    salderingHint,
  } from '../../lib/format/settlement';
  import Tooltip from '../ui/Tooltip.svelte';

  const rows = [
    { label: 'VAT on energy', value: `${(VAT_NL * 100).toFixed(0)} %`, source: 'contract.rs' },
    {
      label: 'Supplier fee (incl. VAT)',
      value: `€ ${FEE_INCL.toFixed(4)}/kWh`,
      source: 'contract.rs',
    },
    { label: 'Feed-in fee', value: `€ ${FEED_IN_FEE.toFixed(4)}/kWh`, source: 'contract.rs' },
    {
      label: 'Terugleverkosten',
      value: `€ ${TERUGLEVER_KOSTEN.toFixed(2)}/kWh`,
      source: 'contract.rs',
    },
    {
      label: 'Energiebelasting 2026',
      value: `€ ${ENERGY_TAX_2026_EX.toFixed(5)}/kWh`,
      source: 'tax_year.rs',
    },
    {
      label: 'Zonnebonus',
      value: `${(BONUS_PCT * 100).toFixed(0)} % · cap ${BONUS_CAP_KWH} kWh/yr`,
      source: 'bonus.rs',
    },
    {
      label: 'Saldering ends',
      value: `${SALDERING_END_YEAR}-01-01`,
      source: 'net_metering.rs',
    },
  ];
</script>

<div class="space-y-2">
  <div class="flex items-center gap-1.5">
    <p class="text-[11px] text-muted">Build-up</p>
    <Tooltip label="How the published price is built">
      <span class="font-mono text-[10px]">
        buy = (market + energiebelasting) × (1 + vat) + fee
      </span>
      <br />
      <span class="font-mono text-[10px]">
        feed-in = (market + feed-in fee) × bonus − terugleverkosten
      </span>
    </Tooltip>
  </div>

  <dl class="divide-y divide-line/70 rounded-lg border border-line">
    {#each rows as row (row.label)}
      <div class="flex items-center justify-between gap-2 px-2.5 py-1.5">
        <dt class="min-w-0 text-[11px] text-muted">
          {row.label}
          <span class="ml-1 font-mono text-[9.5px] text-faint">{row.source}</span>
        </dt>
        <dd class="num shrink-0 text-[11.5px] text-ink">{row.value}</dd>
      </div>
    {/each}
  </dl>

  <p class="flex items-start gap-1.5 text-[10px] leading-relaxed text-faint">
    <Scale class="mt-[1px] size-3 shrink-0" aria-hidden="true" />
    {salderingHint(new Date())}
    These are the contract constants compiled into the engine — the v1 API exposes no endpoint to read
    or write them, so this panel is a mirror of the source files named above, not a setting.
  </p>
</div>
