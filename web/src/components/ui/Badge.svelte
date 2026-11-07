<script lang="ts">
  import type { Snippet } from 'svelte';

  import { BADGE, type Tone } from '../../lib/uiTone';

  interface Props {
    tone?: Tone;
    title?: string;
    icon?: Snippet;
    children: Snippet;
  }

  let { tone = 'neutral', title, icon, children }: Props = $props();
</script>

<span
  class={[
    'inline-flex items-center gap-1 rounded-full border px-2 py-[3px] text-[10.5px] font-medium uppercase tracking-[0.06em]',
    BADGE[tone],
  ]}
  {title}
>
  {#if icon}
    <span class="grid size-3 place-items-center" aria-hidden="true">{@render icon()}</span>
  {/if}
  {@render children()}
</span>
