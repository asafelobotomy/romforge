<script>
  // How many copies of each save folder romforge keeps outside `Saves/`
  // (the data trees of RPCS3, Cemu, the 3DS emulators and Dolphin's Wii NAND).
  let { value = $bindable(5) } = $props();
</script>

<section class="setting">
  <label class="flabel" for="save-copies">Copies of each save</label>
  <select id="save-copies" bind:value>
    {#each [1, 3, 5, 10, 20] as n}<option value={n}>{n}</option>{/each}
    <!-- A number set with the CLI stays as it is. -->
    {#if ![1, 3, 5, 10, 20].includes(value)}<option {value}>{value}</option>{/if}
  </select>
  <p class="hint">Saves that have to stay inside an emulator's own data are copied into Saves/ after a game ends. This many copies of each stay, newest first, so a broken save doesn't replace the good one.</p>
</section>
