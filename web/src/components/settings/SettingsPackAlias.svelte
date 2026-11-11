<script lang="ts">
  import { describeError } from '../../lib/api/errors';
  import { api } from '../../lib/state/runtime';
  import { toasts } from '../../lib/state/toast.svelte';
  import Button from '../ui/Button.svelte';

  interface Props {
    sn: string;
    alias: string | null;
    onchanged: () => void;
  }

  let { sn, alias, onchanged }: Props = $props();

  // The stored alias seeds the field; from then on it is a local draft, and a successful save
  // mirrors the normalised answer back into it. The prop is not meant to drive it continuously.
  // svelte-ignore state_referenced_locally
  let draft = $state(alias ?? '');
  let busy = $state(false);
  const normalized = $derived(draft.trim());
  const dirty = $derived(normalized !== (alias ?? ''));

  async function save(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!dirty) return;
    busy = true;
    try {
      const stored = await api.setPackAlias(sn, normalized === '' ? null : normalized);
      // The server trims and stores; mirror it so the field shows exactly what was kept.
      draft = stored.alias ?? '';
      toasts.push(
        'ok',
        stored.alias === null ? 'Alias cleared' : 'Alias saved',
        `PATCH /packs/${sn}`,
      );
      onchanged();
    } catch (cause) {
      toasts.push('error', 'The alias was refused', describeError(cause));
    } finally {
      busy = false;
    }
  }
</script>

<form class="mt-2 flex items-center gap-1.5" onsubmit={(event) => void save(event)}>
  <input
    type="text"
    bind:value={draft}
    maxlength="40"
    placeholder="Name this pack (alias)"
    aria-label="Alias for pack {sn}"
    class="min-w-0 flex-1 rounded-md border border-line bg-panel px-2 py-1 text-[11.5px] text-ink placeholder:text-faint focus:border-line-strong focus:outline-none"
  />
  <Button
    type="submit"
    variant={dirty ? 'primary' : 'ghost'}
    {busy}
    disabled={!dirty}
    title="PATCH /packs/{sn} with the new alias"
  >
    {normalized === '' && (alias ?? '') !== '' ? 'Clear' : 'Save'}
  </Button>
</form>
