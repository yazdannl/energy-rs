<script lang="ts">
  import { fmtPct } from '../../lib/format/energy';
  import { socTone } from '../../lib/format/sign';

  interface Props {
    pct: number | null;
    size?: number;
    label?: string;
    caption?: string;
    tone?: 'good' | 'warn' | 'bad' | 'muted';
  }

  let { pct, size = 74, label, caption, tone }: Props = $props();

  const stroke = 6;
  const radius = $derived((size - stroke) / 2);
  const circumference = $derived(2 * Math.PI * radius);
  const filled = $derived(pct === null ? 0 : Math.max(0, Math.min(100, pct)));
  const offset = $derived(circumference * (1 - filled / 100));
  const resolvedTone = $derived(tone ?? socTone(pct));
  const toneClass = $derived(
    resolvedTone === 'good'
      ? 'text-good'
      : resolvedTone === 'warn'
        ? 'text-warn'
        : resolvedTone === 'bad'
          ? 'text-bad'
          : 'text-faint',
  );
</script>

<div class="relative grid place-items-center" style="width:{size}px;height:{size}px">
  <svg width={size} height={size} viewBox="0 0 {size} {size}" class="-rotate-90" aria-hidden="true">
    <circle
      cx={size / 2}
      cy={size / 2}
      r={radius}
      fill="none"
      stroke="var(--ui-line)"
      stroke-width={stroke}
    />
    <circle
      cx={size / 2}
      cy={size / 2}
      r={radius}
      fill="none"
      stroke="currentColor"
      class={toneClass}
      stroke-width={stroke}
      stroke-linecap="round"
      stroke-dasharray={circumference}
      stroke-dashoffset={offset}
      style="transition: stroke-dashoffset 700ms cubic-bezier(0.22, 1, 0.36, 1)"
    />
  </svg>
  <div class="absolute grid place-items-center text-center leading-none">
    <span class="num text-[15px] font-semibold text-ink">{fmtPct(pct)}</span>
    {#if label}
      <span class="mt-[3px] text-[9.5px] uppercase tracking-[0.08em] text-faint">{label}</span>
    {/if}
    {#if caption}
      <span class="mt-[2px] text-[9.5px] text-faint">{caption}</span>
    {/if}
  </div>
</div>
