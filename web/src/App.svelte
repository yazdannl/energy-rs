<script lang="ts">
  import { onMount } from 'svelte';

  type Health = { status: string; version: string };

  let health = $state<Health | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    const controller = new AbortController();

    void (async () => {
      try {
        const response = await fetch('./api/v1/health', {
          headers: { accept: 'application/json' },
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        health = (await response.json()) as Health;
      } catch (cause) {
        if (controller.signal.aborted) return;
        error = cause instanceof Error ? cause.message : String(cause);
      }
    })();

    return () => controller.abort();
  });
</script>

<div class="min-h-dvh bg-slate-950 text-slate-100">
  <header class="border-b border-slate-800/80">
    <div class="mx-auto flex max-w-3xl items-center justify-between gap-3 px-5 py-4">
      <div class="flex items-center gap-3">
        <span
          class="grid size-9 place-items-center rounded-lg bg-emerald-500/15 text-emerald-400"
          aria-hidden="true"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            class="size-5"
          >
            <path
              d="M13 2 4.5 13.5H11l-1 8.5 8.5-11.5H12z"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </span>
        <div>
          <h1 class="text-base font-semibold tracking-tight">Energy RS</h1>
          <p class="text-xs text-slate-400">Battery planning for Home Assistant</p>
        </div>
      </div>
      <span
        class="rounded-full border border-slate-700 px-2.5 py-1 text-[11px] uppercase tracking-wide text-slate-400"
      >
        scaffold
      </span>
    </div>
  </header>

  <main class="mx-auto max-w-3xl px-5 py-8">
    <section
      class="rounded-xl border border-slate-800 bg-slate-900/60 p-5"
      aria-labelledby="health-heading"
    >
      <h2 id="health-heading" class="text-sm font-medium text-slate-300">API health</h2>
      <div class="mt-3 text-sm">
        {#if error}
          <p class="text-rose-400">unreachable: {error}</p>
        {:else if health}
          <p class="flex items-center gap-2 text-emerald-400">
            <span class="size-2 rounded-full bg-emerald-400" aria-hidden="true"></span>
            {health.status} · v{health.version}
          </p>
        {:else}
          <p class="text-slate-400">checking…</p>
        {/if}
      </div>
    </section>

    <p class="mt-6 text-xs text-slate-500">
      Screens (Now, Plan, Prices, Batteries, Savings, Settings) arrive in P9.
    </p>
  </main>
</div>
