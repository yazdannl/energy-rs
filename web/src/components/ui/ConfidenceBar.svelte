<script lang="ts">
  import { fmtConfidence } from '../../lib/format/energy';

  interface Props {
    label: string;
    value: number | null;
    detail?: string;
  }

  let { label, value, detail }: Props = $props();

  const tone = $derived(
    value === null ? 'bg-faint' : value >= 0.7 ? 'bg-good' : value >= 0.4 ? 'bg-warn' : 'bg-bad',
  );
  const width = $derived(value === null ? 0 : Math.max(2, Math.min(100, value * 100)));
</script>

<div class="space-y-1">
  <div class="flex items-baseline justify-between gap-2">
    <span class="text-[11px] text-muted">{label}</span>
    <span class="num text-[11px] font-medium text-ink">{fmtConfidence(value)}</span>
  </div>
  <div class="h-[4px] overflow-hidden rounded-full bg-line">
    <div
      class={['h-full rounded-full transition-all duration-500', tone]}
      style="width:{width}%"
    ></div>
  </div>
  {#if detail}
    <p class="text-[10.5px] text-faint">{detail}</p>
  {/if}
</div>
