<script lang="ts">
  import Info from '@lucide/svelte/icons/info';
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    children: Snippet;
    /** Icon-only trigger, for dense rows. */
    compact?: boolean;
  }

  let { label, children, compact = true }: Props = $props();
</script>

<span class="group relative inline-flex items-center">
  <button
    type="button"
    class="inline-flex items-center gap-1 rounded text-faint transition-colors hover:text-ink focus-visible:text-ink focus-visible:outline-none"
    aria-label={label}
  >
    {#if compact}
      <Info class="size-3.5" aria-hidden="true" />
    {:else}
      <span class="text-[11px] underline decoration-dotted underline-offset-2">{label}</span>
    {/if}
  </button>
  <span
    role="tooltip"
    class="pointer-events-none absolute bottom-full left-1/2 z-30 mb-1.5 hidden w-60 -translate-x-1/2 rounded-lg border border-line-strong bg-panel px-3 py-2 text-left text-[11px] leading-relaxed text-muted shadow-xl group-focus-within:block group-hover:block"
  >
    {@render children()}
  </span>
</span>
