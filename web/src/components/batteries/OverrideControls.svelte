<script lang="ts">
  import { onMount } from 'svelte';

  import { describeError } from '../../lib/api/errors';
  import type { OverrideAction, Pack } from '../../lib/api/types';
  import { fmtW } from '../../lib/format/energy';
  import { clock } from '../../lib/format/time';
  import { api } from '../../lib/state/runtime';
  import { toasts } from '../../lib/state/toast.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Segmented from '../ui/Segmented.svelte';

  interface Props {
    pack: Pack;
    onchanged: () => void;
  }

  let { pack, onchanged }: Props = $props();

  const ceiling = $derived(pack.limits.maxChargeW ?? 800);
  let action = $state<OverrideAction>('hold');
  let powerW = $state<number | null>(null);
  let ttlS = $state(900);
  let busy = $state(false);
  let now = $state(Date.now());

  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 5_000);
    return () => clearInterval(timer);
  });

  const remaining = $derived.by(() => {
    const expires = pack.override?.expiresAt;
    if (!expires) return null;
    const ms = new Date(expires).getTime() - now;
    return ms <= 0 ? 'expired' : `${Math.ceil(ms / 60_000)} min left`;
  });

  async function apply(): Promise<void> {
    busy = true;
    try {
      const result = await api.override({
        pack: pack.sn,
        action,
        powerW: action === 'charge' ? Math.min(powerW ?? ceiling, ceiling) : undefined,
        ttlS,
      });
      toasts.push(
        'ok',
        `Override accepted for ${pack.name ?? pack.sn}`,
        `${result.action} until ${clock(result.expiresAt)}`,
      );
      onchanged();
    } catch (cause) {
      toasts.push('error', 'The override was refused', describeError(cause));
    } finally {
      busy = false;
    }
  }
</script>

<div class="space-y-2">
  <div class="flex items-center justify-between gap-2">
    <p class="text-[10.5px] uppercase tracking-[0.07em] text-faint">Manual override</p>
    {#if pack.override}
      <Badge tone="info">
        {pack.override.action}{pack.override.powerW ? ` ${fmtW(pack.override.powerW)}` : ''} · {remaining}
      </Badge>
    {/if}
  </div>

  <Segmented
    label="Override action for {pack.sn}"
    value={action}
    options={[
      { value: 'charge', label: 'Charge', hint: 'Charge at a fixed power until the TTL expires' },
      { value: 'hold', label: 'Hold', hint: 'Hold at zero power' },
      { value: 'off', label: 'Off', hint: 'Refuse all writes to this pack' },
    ]}
    onselect={(value) => (action = value as OverrideAction)}
  />

  <div class="flex flex-wrap items-end gap-2">
    {#if action === 'charge'}
      <label class="flex-1">
        <span class="block text-[10px] text-faint">Power (max {fmtW(ceiling)})</span>
        <input
          type="number"
          min="50"
          max={ceiling}
          step="50"
          value={powerW ?? ceiling}
          oninput={(event) => (powerW = Number((event.currentTarget as HTMLInputElement).value))}
          class="num mt-0.5 w-full rounded-md border border-line bg-panel px-2 py-1 text-[11.5px] text-ink"
        />
      </label>
    {/if}
    <label class="flex-1">
      <span class="block text-[10px] text-faint">For</span>
      <select
        bind:value={ttlS}
        class="mt-0.5 w-full rounded-md border border-line bg-panel px-2 py-1 text-[11.5px] text-ink"
      >
        <option value={300}>5 min</option>
        <option value={900}>15 min</option>
        <option value={1800}>30 min</option>
        <option value={3600}>60 min</option>
      </select>
    </label>
    <Button variant="primary" size="sm" {busy} onclick={() => void apply()}>
      {busy ? 'Sending' : 'Send override'}
    </Button>
  </div>
  <p class="text-[10px] leading-relaxed text-faint">
    The override is queued to the engine (202) and expires on its own; it never rewrites a limit.
  </p>
</div>
