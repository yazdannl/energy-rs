<script lang="ts">
  import CircleSlash from '@lucide/svelte/icons/circle-slash';
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    detail?: string;
    children?: Snippet;
  }

  let { title, detail, children }: Props = $props();
</script>

<div
  class="flex flex-col items-center gap-2 rounded-lg border border-dashed border-line px-4 py-6 text-center"
>
  <CircleSlash class="size-4 text-faint" aria-hidden="true" />
  <p class="text-[12px] font-medium text-muted">{title}</p>
  {#if detail}
    <p class="max-w-md text-[11px] leading-relaxed text-faint">{detail}</p>
  {/if}
  {#if children}{@render children()}{/if}
</div>
