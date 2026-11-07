<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    subtitle?: string;
    icon?: Snippet;
    actions?: Snippet;
    children: Snippet;
    footer?: Snippet;
    /** Tighter padding for dense grids. */
    compact?: boolean;
    class?: string;
  }

  let {
    title,
    subtitle,
    icon,
    actions,
    children,
    footer,
    compact = false,
    class: extra = '',
  }: Props = $props();
</script>

<section class={['card overflow-hidden', extra]}>
  {#if title || actions || icon}
    <header class="flex flex-wrap items-start justify-between gap-3 px-4 pt-3.5 pb-2">
      <div class="flex min-w-0 items-center gap-2.5">
        {#if icon}
          <span class="grid size-7 shrink-0 place-items-center rounded-lg" aria-hidden="true">
            {@render icon()}
          </span>
        {/if}
        <div class="min-w-0">
          {#if title}
            <h2 class="truncate text-[13px] font-semibold tracking-wide text-ink">{title}</h2>
          {/if}
          {#if subtitle}
            <p class="truncate text-[11px] text-muted">{subtitle}</p>
          {/if}
        </div>
      </div>
      {#if actions}
        <div class="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {@render actions()}
        </div>
      {/if}
    </header>
  {/if}
  <div class={compact ? 'px-4 pb-3.5' : 'px-4 pb-4'}>
    {@render children()}
  </div>
  {#if footer}
    <div class="border-t border-line px-4 py-2.5">{@render footer()}</div>
  {/if}
</section>
