<script>
  import SaveCopiesSetting from "./SaveCopiesSetting.svelte";

  // Settings → Display: what romforge does while a game plays, how often
  // Refresh goes online, the save copies kept and the threads long jobs
  // use. Part of the dialog's draft, saved with Save.
  let { draft = $bindable() } = $props();
</script>

  <section class="setting">
    <label class="flabel" for="while-playing">While a game plays</label>
    <select id="while-playing" bind:value={draft.while_playing}>
      <option value="tray">Hide romforge to the tray</option>
      <option value="minimise">Minimise romforge</option>
      <option value="stay">Keep romforge open</option>
    </select>
    <p class="hint">Once the game is up. romforge comes back by itself when the game ends; from the tray, Show romforge brings it back sooner and Stop asks the game to quit. Without a tray (some desktops have none), romforge minimises instead.</p>
  </section>

  <section class="setting">
    <label class="flabel" for="list-age">Online lists on Refresh</label>
    <select id="list-age" bind:value={draft.list_age_days}>
      {#each [[7, "Every 7 days"], [14, "Every 14 days"], [30, "Every 30 days"], [0, "Never"]] as [d, label]}<option value={d}>{label}</option>{/each}
      <!-- A number set with the CLI stays as it is. -->
      {#if ![7, 14, 30, 0].includes(draft.list_age_days)}<option value={draft.list_age_days}>Every {draft.list_age_days} days</option>{/if}
    </select>
    <p class="hint">Refresh first reads ScummVM's and MAME's lists again when the installed program changed, and downloads the DATs and lists of the systems you have files for once they are this old. Never: Refresh doesn't go online for them (Download DATs still does).</p>
  </section>

  <SaveCopiesSetting bind:value={draft.save_copies} />

  <section class="setting">
    <label class="flabel" for="threads">Threads for long jobs</label>
    <select id="threads" bind:value={draft.threads}>
      <option value={0}>Automatic (one per CPU, at most 8)</option>
      <option value={1}>1 (a slow spinning disk)</option>
      {#each [2, 4, 8] as n}<option value={n}>{n}</option>{/each}
    </select>
    <p class="hint">How many files Scan and Refresh hash, and how many games Rename & Cleanup and Convert write, at once; downloads use up to four. Each website still gets no more than its own pace.</p>
  </section>
