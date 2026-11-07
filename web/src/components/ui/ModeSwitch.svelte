<script lang="ts">
  import Eye from '@lucide/svelte/icons/eye';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import Zap from '@lucide/svelte/icons/zap';

  import { describeError } from '../../lib/api/errors';
  import type { Mode } from '../../lib/api/types';
  import { api } from '../../lib/state/runtime';
  import { connection } from '../../lib/state/connection.svelte';
  import { toasts } from '../../lib/state/toast.svelte';
  import Button from '../ui/Button.svelte';

  interface Props {
    mode: Mode | null;
  }

  let { mode }: Props = $props();

  let pending = $state<Mode | null>(null);
  let busy = $state(false);

  const OPTIONS: { value: Mode; label: string; hint: string; icon: typeof Eye }[] = [
    { value: 'advisory', label: 'Advisory', hint: 'Compute and log; never write.', icon: Eye },
    {
      value: 'dry_run',
      label: 'Dry run',
      hint: 'Full pipeline, journal only; never write.',
      icon: FlaskConical,
    },
    {
      value: 'auto',
      label: 'Auto',
      hint: 'May write to the packs. Two taps to confirm.',
      icon: Zap,
    },
  ];

  async function choose(next: Mode): Promise<void> {
    if (next === mode) return;
    if (next === 'auto' && pending !== 'auto') {
      pending = 'auto';
      return;
    }
    pending = null;
    busy = true;
    try {
      const result = await api.setMode(next);
      connection.noteMode(result.mode);
      toasts.push(
        result.engineNotified ? 'ok' : 'info',
        `Mode set to ${result.mode}`,
        result.engineNotified ? undefined : 'Saved, but no engine loop is running to receive it.',
      );
    } catch (cause) {
      toasts.push('error', 'Could not change the mode', describeError(cause));
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex flex-wrap items-center gap-1.5">
  {#each OPTIONS as option (option.value)}
    {@const active = mode === option.value}
    {@const confirming = pending === option.value}
    <Button
      variant={active ? 'primary' : confirming ? 'danger' : 'ghost'}
      {busy}
      title={option.hint}
      onclick={() => void choose(option.value)}
    >
      {#snippet icon()}<option.icon class="size-3.5" />{/snippet}
      {confirming ? 'Confirm auto' : option.label}
    </Button>
  {/each}
</div>
<p class="mt-1.5 text-[10px] leading-relaxed text-faint">
  {#if pending === 'auto'}
    Auto lets the engine write to the packs. Tap confirm to switch; the ledger setting is written
    first.
  {:else}
    The mode is also written to the ledger, so a restart keeps it.
  {/if}
</p>
