<script lang="ts">
  import type { Snippet } from 'svelte';

  import { TEXT, type Tone } from '../../lib/uiTone';

  interface Props {
    label: string;
    value: string;
    unit?: string;
    hint?: string;
    tone?: Tone;
    icon?: Snippet;
    exact?: string;
  }

  let { label, value, unit, hint, tone = 'neutral', icon, exact }: Props = $props();
</script>

<div class="rounded-lg border border-line bg-panel-soft/60 px-3 py-2.5">
  <div class="flex items-center gap-1.5 text-[10.5px] uppercase tracking-[0.07em] text-faint">
    {#if icon}<span class="grid size-3 place-items-center">{@render icon()}</span>{/if}
    <span class="truncate">{label}</span>
  </div>
  <div class="mt-1 flex items-baseline gap-1" title={exact}>
    <span class={['num text-[17px] leading-none font-semibold', TEXT[tone]]}>{value}</span>
    {#if unit}<span class="text-[11px] text-faint">{unit}</span>{/if}
  </div>
  {#if hint}<p class="mt-1 truncate text-[10.5px] text-muted">{hint}</p>{/if}
</div>
