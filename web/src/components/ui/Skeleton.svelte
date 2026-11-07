<script lang="ts">
  interface Props {
    /** Number of placeholder rows. */
    rows?: number;
    height?: string;
    class?: string;
  }

  let { rows = 3, height = 'h-4', class: extra = '' }: Props = $props();
</script>

<div class={['animate-pulse space-y-2', extra]} aria-busy="true" aria-live="polite">
  {#each Array.from({ length: rows }, (_, index) => index) as row (row)}
    <div
      class={['rounded-md bg-line/70', height]}
      style="width:{row === rows - 1 ? 62 : 100 - row * 8}%"
    ></div>
  {/each}
  <span class="sr-only">Loading</span>
</div>
