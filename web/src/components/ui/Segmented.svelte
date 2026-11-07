<script lang="ts">
  interface Option {
    value: string;
    label: string;
    hint?: string;
  }

  interface Props {
    options: Option[];
    value: string;
    label: string;
    disabled?: boolean;
    onselect: (value: string) => void;
  }

  let { options, value, label, disabled = false, onselect }: Props = $props();
</script>

<div
  class="inline-flex rounded-lg border border-line bg-panel-soft p-[2px]"
  role="group"
  aria-label={label}
>
  {#each options as option (option.value)}
    <button
      type="button"
      aria-pressed={value === option.value}
      {disabled}
      title={option.hint}
      onclick={() => onselect(option.value)}
      class={[
        'rounded-md px-2.5 py-[3px] text-[11.5px] font-medium transition-colors duration-150',
        value === option.value
          ? 'bg-ink/10 text-ink shadow-[inset_0_0_0_1px_var(--ui-line-strong)]'
          : 'text-muted hover:text-ink',
      ]}
    >
      {option.label}
    </button>
  {/each}
</div>
