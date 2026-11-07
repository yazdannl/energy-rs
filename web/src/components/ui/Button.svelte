<script lang="ts">
  import type { Snippet } from 'svelte';

  import { BUTTON } from '../../lib/uiTone';

  interface Props {
    variant?: 'primary' | 'ghost' | 'danger';
    size?: 'sm' | 'md';
    type?: 'button' | 'submit';
    disabled?: boolean;
    busy?: boolean;
    title?: string;
    icon?: Snippet;
    onclick?: (event: MouseEvent) => void;
    children?: Snippet;
  }

  let {
    variant = 'ghost',
    size = 'sm',
    type = 'button',
    disabled = false,
    busy = false,
    title,
    icon,
    onclick,
    children,
  }: Props = $props();

  const sizes = {
    sm: 'h-7 gap-1.5 px-2.5 text-[11.5px]',
    md: 'h-9 gap-2 px-3.5 text-[13px]',
  };
</script>

<button
  {type}
  {title}
  disabled={disabled || busy}
  {onclick}
  class={[
    'inline-flex items-center justify-center rounded-lg font-medium transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-45',
    sizes[size],
    BUTTON[variant],
  ]}
>
  {#if busy}
    <span
      class="size-3 animate-spin rounded-full border border-current border-t-transparent"
      aria-hidden="true"
    ></span>
  {:else if icon}
    <span class="grid size-3.5 place-items-center" aria-hidden="true">{@render icon()}</span>
  {/if}
  {#if children}{@render children()}{/if}
</button>
