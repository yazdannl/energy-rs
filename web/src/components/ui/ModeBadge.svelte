<script lang="ts">
  import Eye from '@lucide/svelte/icons/eye';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import Zap from '@lucide/svelte/icons/zap';

  import type { Mode } from '../../lib/api/types';
  import { BADGE, type Tone } from '../../lib/uiTone';

  interface Props {
    mode: Mode | null;
    size?: 'sm' | 'md';
  }

  let { mode, size = 'sm' }: Props = $props();

  const TONE: Record<Mode, Tone> = { advisory: 'warn', dry_run: 'info', auto: 'good' };
  const LABEL: Record<Mode, string> = {
    advisory: 'Advisory',
    dry_run: 'Dry run',
    auto: 'Auto',
  };
  const TITLE: Record<Mode, string> = {
    advisory: 'Computes and logs a plan; never writes to a device.',
    dry_run: 'Runs the whole pipeline and journals the intended write; never writes to a device.',
    auto: 'May write to the packs.',
  };
</script>

{#if mode === null}
  <span
    class={[
      'inline-flex items-center gap-1 rounded-full border px-2 py-[3px] text-[10.5px] uppercase tracking-[0.06em]',
      BADGE.neutral,
    ]}
  >
    unknown mode
  </span>
{:else}
  <span
    class={[
      'inline-flex items-center gap-1 rounded-full border font-medium uppercase tracking-[0.06em]',
      BADGE[TONE[mode]],
      size === 'md' ? 'px-2.5 py-1 text-[11px]' : 'px-2 py-[3px] text-[10.5px]',
    ]}
    title={TITLE[mode]}
  >
    {#if mode === 'auto'}
      <Zap class="size-3" aria-hidden="true" />
    {:else if mode === 'dry_run'}
      <FlaskConical class="size-3" aria-hidden="true" />
    {:else}
      <Eye class="size-3" aria-hidden="true" />
    {/if}
    {LABEL[mode]}
  </span>
{/if}
