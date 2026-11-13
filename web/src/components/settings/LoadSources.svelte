<script lang="ts">
  let {
    loadEntity = $bindable(''),
    temperatureEntity = $bindable(''),
    weatherEntity = $bindable(''),
  } = $props();

  const input =
    'mt-1 w-full rounded-md border border-line bg-panel px-2 py-1 font-mono text-[11px] text-ink focus:border-line-strong focus:outline-none';
</script>

<label class="block">
  <span class="text-[11px] text-muted">House load meter</span>
  <input type="text" placeholder="sensor.home_load_power" bind:value={loadEntity} class={input} />
  <span class="mt-0.5 block text-[10px] text-faint">
    A W or kWh meter for the whole house. Empty learns from the energy dashboard's balance (import −
    export + PV + discharge − charge), which is only as good as its counters.
  </span>
</label>

<div class="grid gap-2 sm:grid-cols-2">
  <label class="block">
    <span class="text-[11px] text-muted">Outdoor temperature</span>
    <input
      type="text"
      placeholder="sensor.outdoor_temperature"
      bind:value={temperatureEntity}
      class={input}
    />
    <span class="mt-0.5 block text-[10px] text-faint">
      A °C sensor; its history fits the house and heat-pump temperature terms.
    </span>
  </label>
  <label class="block">
    <span class="text-[11px] text-muted">Weather forecast</span>
    <input type="text" placeholder="weather.home" bind:value={weatherEntity} class={input} />
    <span class="mt-0.5 block text-[10px] text-faint">
      Hourly temperatures for the plan horizon; without it the measured hour is held flat.
    </span>
  </label>
</div>
