<script lang="ts">
  import Zap from '@lucide/svelte/icons/zap';

  import { ago } from '../../lib/format/time';
  import { connection } from '../../lib/state/connection.svelte';
  import ConnectionPill from '../ui/ConnectionPill.svelte';
  import ModeBadge from '../ui/ModeBadge.svelte';

  interface Props {
    title: string;
  }

  let { title }: Props = $props();
</script>

<header class="sticky top-0 z-30 border-b border-line bg-base/80 backdrop-blur-md">
  <div class="mx-auto flex w-full max-w-[1500px] items-center gap-3 px-4 py-2.5">
    <a href="#/" class="flex items-center gap-2 md:hidden">
      <span
        class="grid size-7 place-items-center rounded-lg bg-good/12 text-good"
        aria-hidden="true"
      >
        <Zap class="size-4" />
      </span>
    </a>
    <div class="min-w-0 flex-1">
      <p class="truncate text-[12.5px] font-medium text-ink">{title}</p>
      <p class="truncate text-[10.5px] text-faint">
        {#if connection.snapshot === null}
          waiting for the first tick
        {:else}
          {connection.snapshot.mode} · tick {ago(connection.snapshot.lastTickAt)}
        {/if}
      </p>
    </div>
    <div class="hidden sm:block">
      <ModeBadge mode={connection.snapshot?.mode ?? null} />
    </div>
    <ConnectionPill />
  </div>
</header>
