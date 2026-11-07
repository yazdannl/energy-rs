<script lang="ts">
  import TriangleAlert from '@lucide/svelte/icons/triangle-exclamation-point';
  import Check from '@lucide/svelte/icons/check';
  import Info from '@lucide/svelte/icons/info';
  import X from '@lucide/svelte/icons/x';
  import { fly } from 'svelte/transition';

  import { duration } from '../../lib/state/motion.svelte';
  import { toasts, type ToastKind } from '../../lib/state/toast.svelte';

  const STYLE: Record<ToastKind, { box: string; icon: typeof Check }> = {
    ok: { box: 'border-good/40 text-good', icon: Check },
    error: { box: 'border-bad/40 text-bad', icon: TriangleAlert },
    info: { box: 'border-batt/40 text-batt', icon: Info },
  };
</script>

<div
  class="pointer-events-none fixed inset-x-0 bottom-20 z-50 flex flex-col items-center gap-2 px-4 md:inset-x-auto md:right-6 md:bottom-6 md:items-end"
>
  {#each toasts.items as toast (toast.id)}
    {@const Icon = STYLE[toast.kind].icon}
    <div
      class={[
        'card pointer-events-auto flex w-full max-w-sm items-start gap-2.5 border px-3.5 py-2.5 shadow-2xl',
        STYLE[toast.kind].box,
      ]}
      role="status"
      transition:fly={{ y: 8, duration: duration(160) }}
    >
      <Icon class="mt-[1px] size-4 shrink-0" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <p class="text-[12px] font-medium text-ink">{toast.message}</p>
        {#if toast.detail}
          <p class="mt-0.5 text-[11px] leading-relaxed text-muted">{toast.detail}</p>
        {/if}
      </div>
      <button
        type="button"
        class="rounded p-0.5 text-faint hover:text-ink"
        aria-label="Dismiss"
        onclick={() => toasts.dismiss(toast.id)}
      >
        <X class="size-3.5" aria-hidden="true" />
      </button>
    </div>
  {/each}
</div>
