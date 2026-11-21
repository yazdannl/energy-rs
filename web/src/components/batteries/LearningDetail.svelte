<script lang="ts">
  import { fmtKwh } from '../../lib/format/energy';
  import { fmtConfidence } from '../../lib/format/energy';
  import { ago } from '../../lib/format/time';
  import type { PackLearned } from '../../lib/api/types';
  import Badge from '../ui/Badge.svelte';
  import ConfidenceBar from '../ui/ConfidenceBar.svelte';
  import Tooltip from '../ui/Tooltip.svelte';

  interface Props {
    learned: PackLearned;
  }

  let { learned }: Props = $props();

  const band = $derived(
    learned.capacityBandLowKwh !== null && learned.capacityBandHighKwh !== null
      ? `${fmtKwh(learned.capacityBandLowKwh)} – ${fmtKwh(learned.capacityBandHighKwh)}`
      : null,
  );
</script>

<div class="space-y-2.5">
  <div class="flex items-center justify-between gap-2">
    <p class="text-[10.5px] uppercase tracking-[0.07em] text-faint">Learned</p>
    <div class="flex items-center gap-1">
      {#if learned.capacityApplied}
        <Badge tone="good">applied</Badge>
      {:else}
        <Badge tone="warn">not applied</Badge>
      {/if}
      <Tooltip label="Where this capacity comes from">
        {#if learned.capacityDecision}
          <span class="font-mono text-[10px] text-ink">{learned.capacityDecision}</span>
          <br />
        {/if}
        {learned.cyclesUsed ?? learned.sessionsUsed ?? 0} usable cycle{(learned.cyclesUsed ??
          learned.sessionsUsed ??
          0) === 1
          ? ''
          : 's'},
        {(learned.socPointsCovered ?? 0).toFixed(0)} SoC points, source {learned.capacitySource ??
          'label'}, updated {ago(learned.updatedAt)}. Confidence comes from covered SoC and span
        length, not session count; a pooled value means this pack had too little evidence of its
        own.
      </Tooltip>
    </div>
  </div>

  <div class="flex items-end justify-between gap-2">
    <div>
      <p class="num text-[18px] leading-none font-semibold text-ink">
        {fmtKwh(learned.capacityKwh)}
      </p>
      <p class="mt-0.5 text-[10px] text-faint">
        {band ? `band ${band}` : 'no band'} · label-vs-learned is not assumed
      </p>
    </div>
    <div class="text-right">
      <p class="num text-[12px] text-ink">{fmtConfidence(learned.efficiencyConfidence)}</p>
      <p class="text-[10px] text-faint">efficiency conf.</p>
    </div>
  </div>

  <ConfidenceBar label="Capacity confidence" value={learned.capacityConfidence} />
  <ConfidenceBar
    label="Efficiency confidence"
    value={learned.efficiencyConfidence}
    detail={learned.roundTrip === null
      ? 'No round-trip measurement yet'
      : `${learned.roundTripSource === 'measured' ? 'measured' : 'typed default, no closed cycle yet'} · round trip ${(learned.roundTrip * 100).toFixed(1)} % · η ${learned.etaCharge?.toFixed(3) ?? '—'} each way`}
  />
</div>
