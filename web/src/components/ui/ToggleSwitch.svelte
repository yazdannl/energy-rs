<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    hint?: string;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  }

  let { checked, label, hint, disabled = false, onchange }: Props = $props();
</script>

<label class="flex cursor-pointer items-center gap-2 select-none {disabled ? 'opacity-50' : ''}">
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    {disabled}
    onclick={() => onchange(!checked)}
    class={[
      'relative h-[18px] w-[32px] shrink-0 rounded-full border transition-colors duration-150',
      checked ? 'border-good/50 bg-good/30' : 'border-line-strong bg-panel-soft',
    ]}
  >
    <span
      class={[
        'absolute top-[2px] size-[12px] rounded-full transition-all duration-150',
        checked ? 'left-[16px] bg-good' : 'left-[2px] bg-muted',
      ]}
    ></span>
  </button>
  <span class="text-[11.5px] text-ink">{label}</span>
  {#if hint}
    <span class="text-[10.5px] text-faint">{hint}</span>
  {/if}
</label>
