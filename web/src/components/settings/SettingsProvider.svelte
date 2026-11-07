<script lang="ts">
  import KeyRound from '@lucide/svelte/icons/key-round';
  import Lock from '@lucide/svelte/icons/lock';

  import type { SettingValue } from '../../lib/api/types';
  import type { SettingsDraft } from '../../lib/state/settingsDraft.svelte';
  import Tooltip from '../ui/Tooltip.svelte';

  interface Props {
    draft: SettingsDraft;
  }

  let { draft }: Props = $props();

  const provider = $derived(draft.entry('price_provider'));
  const providers = $derived(provider?.options ?? ['zonneplan', 'ha']);

  function setText(key: string, event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    const payload: SettingValue = value === '' ? null : value;
    draft.set(key, payload);
  }
</script>

<div class="space-y-2.5">
  <label class="block">
    <span class="text-[11px] text-muted">Price provider</span>
    <select
      value={draft.text('price_provider')}
      onchange={(event) =>
        draft.set('price_provider', (event.currentTarget as HTMLSelectElement).value)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 text-[12px] text-ink focus:border-line-strong focus:outline-none"
    >
      {#each providers as option (option)}
        <option value={option}>{option}</option>
      {/each}
    </select>
  </label>

  <label class="block">
    <span class="text-[11px] text-muted">Price entity</span>
    <input
      type="text"
      value={draft.text('price_entity')}
      placeholder="sensor.zonneplan_current_electricity_tariff"
      oninput={(event) => setText('price_entity', event)}
      class="mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 font-mono text-[11.5px] text-ink focus:border-line-strong focus:outline-none"
    />
    <span class="mt-0.5 block text-[9.5px] text-faint"
      >Home Assistant entity behind the provider.</span
    >
  </label>

  <div class="rounded-lg border border-line bg-panel-soft/50 px-2.5 py-2">
    <span class="flex items-center gap-1.5 text-[11px] text-muted">
      <KeyRound class="size-3.5 text-faint" aria-hidden="true" />
      Provider token
      <Tooltip label="Why this field is disabled">
        The v1 contract has no token endpoint: <span class="font-mono">GET /settings</span> never
        returns a token and <span class="font-mono">PATCH /settings</span> answers
        <span class="font-mono">unsupported_setting</span> for it. The field is write-only by being absent
        — nothing is fetched, echoed or stored in the page.
      </Tooltip>
    </span>
    <div class="relative mt-1">
      <input
        type="password"
        value=""
        readonly
        disabled
        aria-label="Provider token (not available through the v1 API)"
        placeholder="not exposed by the API"
        autocomplete="off"
        class="w-full cursor-not-allowed rounded-md border border-line bg-panel px-2 py-1 pr-7 text-[11.5px] text-faint"
      />
      <Lock class="absolute top-1.5 right-2 size-3.5 text-faint" aria-hidden="true" />
    </div>
    <p class="mt-1 text-[9.5px] leading-relaxed text-faint">
      Set the token in the add-on options. The UI never reads it back: an existing secret is not
      fetched, not diffed and not put in a form value.
    </p>
  </div>
</div>
