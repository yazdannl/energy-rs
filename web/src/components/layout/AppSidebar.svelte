<script lang="ts">
  import Zap from '@lucide/svelte/icons/zap';

  import { router } from '../../lib/state/router.svelte';
  import { mockMode } from '../../lib/state/runtime';
  import ConnectionPill from '../ui/ConnectionPill.svelte';
  import ModeBadge from '../ui/ModeBadge.svelte';
  import { connection } from '../../lib/state/connection.svelte';
  import { NAV_ITEMS } from './navigation';
</script>

<aside class="sticky top-0 hidden h-dvh w-52 shrink-0 flex-col border-r border-line py-4 md:flex">
  <a href="#/" class="flex items-center gap-2.5 px-4 pb-5">
    <span class="grid size-8 place-items-center rounded-lg bg-good/12 text-good" aria-hidden="true">
      <Zap class="size-4.5" />
    </span>
    <span class="leading-tight">
      <span class="block text-[13px] font-semibold tracking-tight text-ink">Energy RS</span>
      <span class="block text-[10.5px] text-faint">battery planning</span>
    </span>
  </a>

  <nav class="flex-1 space-y-0.5 px-2" aria-label="Screens">
    {#each NAV_ITEMS as item (item.path)}
      {@const active = router.path === item.path}
      <a
        href="#{item.path}"
        aria-current={active ? 'page' : undefined}
        class={[
          'group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[12.5px] transition-colors duration-150',
          active
            ? 'bg-ink/[0.06] font-medium text-ink shadow-[inset_1px_0_0_0_var(--ui-good)]'
            : 'text-muted hover:bg-ink/[0.03] hover:text-ink',
        ]}
      >
        <item.icon class={`size-4 ${active ? 'text-good' : 'text-faint group-hover:text-muted'}`} />
        {item.label}
      </a>
    {/each}
  </nav>

  <div class="space-y-2 px-4 pt-3">
    <div class="flex items-center justify-between gap-2">
      <ModeBadge mode={connection.snapshot?.mode ?? null} />
      {#if mockMode}
        <span class="text-[10px] text-faint">fixtures</span>
      {/if}
    </div>
    <ConnectionPill />
    <p class="text-[10px] leading-relaxed text-faint">
      Reads the ledger; writes go through the engine command queue.
    </p>
  </div>
</aside>
