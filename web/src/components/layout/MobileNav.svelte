<script lang="ts">
  import { router } from '../../lib/state/router.svelte';
  import { NAV_ITEMS } from './navigation';
</script>

<nav
  class="fixed inset-x-0 bottom-0 z-40 border-t border-line bg-panel/95 backdrop-blur-md md:hidden"
  aria-label="Screens"
>
  <div class="mx-auto flex max-w-lg items-stretch justify-between px-1">
    {#each NAV_ITEMS as item (item.path)}
      {@const active = router.path === item.path}
      <a
        href="#{item.path}"
        aria-current={active ? 'page' : undefined}
        class={[
          'flex flex-1 flex-col items-center gap-0.5 py-1.5 text-[9.5px] transition-colors',
          active ? 'text-good' : 'text-faint',
        ]}
      >
        <item.icon class="size-4.5" />
        {item.label}
      </a>
    {/each}
  </div>
</nav>
