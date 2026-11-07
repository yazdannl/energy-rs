<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    summary: string;
    actions?: Snippet;
  }

  let { title, summary, actions }: Props = $props();
</script>

<header class="flex flex-wrap items-end justify-between gap-3">
  <div class="min-w-0">
    <h1 class="text-[19px] font-semibold tracking-tight text-ink">{title}</h1>
    <p class="mt-0.5 text-[12px] leading-relaxed text-muted">{summary}</p>
  </div>
  {#if actions}
    <div class="flex flex-wrap items-center gap-1.5">{@render actions()}</div>
  {/if}
</header>
