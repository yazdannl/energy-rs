<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';

  import AppSidebar from './components/layout/AppSidebar.svelte';
  import AppTopBar from './components/layout/AppTopBar.svelte';
  import MobileNav from './components/layout/MobileNav.svelte';
  import ToastStack from './components/ui/ToastStack.svelte';
  import { connection } from './lib/state/connection.svelte';
  import { duration } from './lib/state/motion.svelte';
  import { stream } from './lib/state/runtime';
  import { routeOf, router } from './lib/state/router.svelte';
  import BatteriesPage from './routes/BatteriesPage.svelte';
  import NowPage from './routes/NowPage.svelte';
  import PlanPage from './routes/PlanPage.svelte';
  import PricesPage from './routes/PricesPage.svelte';
  import SavingsPage from './routes/SavingsPage.svelte';
  import SettingsPage from './routes/SettingsPage.svelte';

  onMount(() => {
    connection.start(stream);
    return () => connection.stop();
  });

  const route = $derived(routeOf(router.path));
</script>

<div class="min-h-dvh">
  <AppTopBar title={route.title} />

  <div class="mx-auto flex w-full max-w-[1560px]">
    <AppSidebar />
    <main class="min-w-0 flex-1 px-4 pt-4 pb-24 md:px-6 md:pt-5 md:pb-10">
      <svelte:boundary>
        {#key router.path}
          <div in:fade={{ duration: duration(160) }}>
            {#if router.path === '/plan'}
              <PlanPage />
            {:else if router.path === '/prices'}
              <PricesPage />
            {:else if router.path === '/batteries'}
              <BatteriesPage />
            {:else if router.path === '/savings'}
              <SavingsPage />
            {:else if router.path === '/settings'}
              <SettingsPage />
            {:else}
              <NowPage />
            {/if}
          </div>
        {/key}

        {#snippet failed(error, reset)}
          <div class="card mt-4 p-4">
            <p class="text-[12px] font-medium text-bad">This screen failed to render</p>
            <p class="mt-1 font-mono text-[10.5px] leading-relaxed text-muted">
              {error instanceof Error ? error.message : String(error)}
            </p>
            <button
              type="button"
              class="mt-2 rounded-md border border-line-strong px-2 py-1 text-[11px] text-ink"
              onclick={reset}
            >
              Try again
            </button>
          </div>
        {/snippet}
      </svelte:boundary>
    </main>
  </div>

  <MobileNav />
  <ToastStack />
</div>
