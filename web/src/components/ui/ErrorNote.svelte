<script lang="ts">
  import TriangleAlert from '@lucide/svelte/icons/triangle-exclamation-point';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';

  import type { ApiError } from '../../lib/api/errors';
  import Button from './Button.svelte';

  interface Props {
    error: ApiError;
    onretry?: () => void;
  }

  let { error, onretry }: Props = $props();
</script>

<div class="rounded-lg border border-bad/30 bg-bad/5 px-3.5 py-3">
  <div class="flex items-start gap-2.5">
    <TriangleAlert class="mt-[1px] size-4 shrink-0 text-bad" aria-hidden="true" />
    <div class="min-w-0 flex-1">
      <p class="text-[12px] font-medium text-bad">{error.message}</p>
      <p class="mt-0.5 text-[11px] leading-relaxed text-muted">
        {error.hint} <span class="font-mono text-[10px] text-faint">{error.code}</span>
      </p>
    </div>
    {#if onretry}
      <Button onclick={onretry} title="Try again">
        {#snippet icon()}<RefreshCw class="size-3.5" />{/snippet}
        Retry
      </Button>
    {/if}
  </div>
</div>
