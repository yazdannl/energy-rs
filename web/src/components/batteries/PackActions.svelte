<script lang="ts">
  import BatteryCharging from '@lucide/svelte/icons/battery-charging';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';

  import { describeError } from '../../lib/api/errors';
  import type { Pack } from '../../lib/api/types';
  import { api } from '../../lib/state/runtime';
  import { toasts } from '../../lib/state/toast.svelte';
  import Button from '../ui/Button.svelte';

  interface Props {
    pack: Pack;
    onchanged: () => void;
  }

  let { pack, onchanged }: Props = $props();

  let pending = $state<'full_charge' | 'relearn' | null>(null);
  let busy = $state(false);

  async function run(action: 'full_charge' | 'relearn'): Promise<void> {
    if (pending !== action) {
      pending = action;
      return;
    }
    pending = null;
    busy = true;
    try {
      if (action === 'full_charge') await api.fullCharge(pack.sn);
      else await api.relearn(pack.sn);
      toasts.push(
        'ok',
        action === 'full_charge' ? 'Full charge queued' : 'Relearn queued',
        pack.name ?? pack.sn,
      );
      onchanged();
    } catch (cause) {
      toasts.push('error', 'The engine refused the command', describeError(cause));
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex flex-wrap items-center gap-1.5">
  <Button
    variant={pending === 'full_charge' ? 'danger' : 'ghost'}
    {busy}
    title="Top the pack up to 100 % — a deliberate, one-off cycle"
    onclick={() => void run('full_charge')}
  >
    {#snippet icon()}<BatteryCharging class="size-3.5" />{/snippet}
    {pending === 'full_charge' ? 'Confirm full charge' : 'Full charge'}
  </Button>
  <Button
    variant={pending === 'relearn' ? 'danger' : 'ghost'}
    {busy}
    title="Discard the learned capacity and start a fresh learning session"
    onclick={() => void run('relearn')}
  >
    {#snippet icon()}<RotateCcw class="size-3.5" />{/snippet}
    {pending === 'relearn' ? 'Confirm relearn' : 'Relearn'}
  </Button>
</div>
