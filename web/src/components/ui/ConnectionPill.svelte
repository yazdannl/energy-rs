<script lang="ts">
  import Radio from '@lucide/svelte/icons/radio';

  import { ago } from '../../lib/format/time';
  import { connection } from '../../lib/state/connection.svelte';
  import { mockMode } from '../../lib/state/runtime';

  const STATUS = {
    connecting: { label: 'Connecting', dot: 'bg-warn', text: 'text-warn' },
    live: { label: 'Live', dot: 'bg-good', text: 'text-good' },
    reconnecting: { label: 'Reconnecting', dot: 'bg-warn', text: 'text-warn' },
    offline: { label: 'Offline', dot: 'bg-bad', text: 'text-bad' },
  } as const;

  const current = $derived(STATUS[connection.status]);
  const tick = $derived(ago(connection.snapshot?.lastTickAt ?? null));
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full border border-line bg-panel-soft px-2 py-[3px] text-[10.5px] text-muted"
  title={connection.detail ?? 'Server-sent events from the engine'}
>
  <span class="relative grid size-2 place-items-center">
    <span
      class={[
        'absolute size-2 rounded-full opacity-60',
        current.dot,
        connection.status === 'live' ? 'live-dot' : '',
      ]}
    ></span>
    <span class={['size-1 rounded-full', current.dot]}></span>
  </span>
  <span class={current.text}>{current.label}</span>
  {#if mockMode}
    <span
      class="rounded bg-grid/15 px-1 text-[9.5px] font-semibold uppercase tracking-wide text-grid"
      >mock</span
    >
  {:else if connection.status === 'live' && tick !== '—'}
    <span class="text-faint">· {tick}</span>
  {/if}
  <Radio class="size-3 text-faint" aria-hidden="true" />
</span>
