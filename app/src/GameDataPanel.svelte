<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { run } from "./jobs.svelte.js";

  // "Your game data" on an emulator's tab: the data it keeps outside
  // romforge's config (RPCS3's devices, Cemu's MLC, xemu's disk, the 3DS
  // emulators' NAND and SD card), moved into RomForge when asked. The core
  // plans, copies, checks and journals; this only shows and asks.
  let { emulator, label, note = $bindable(""), error = $bindable("") } = $props();

  let plan = $state(null);
  let working = $state(false);
  const size = (b) => (b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GiB` : `${Math.max(1, Math.round(b / (1 << 20)))} MiB`);

  async function load() {
    plan = await invoke("move_data_plan", { emulator }).catch((e) => {
      error = String(e);
      return null;
    });
  }
  $effect(() => {
    emulator;
    load();
  });

  async function move() {
    const ok = await ask(
      `Move ${label}'s game data (${size(plan.bytes)}) into RomForge? Each file is copied and checked first; your originals then go to RomForge's trash, so Undo can put them back until the trash is emptied.`,
      { title: "Move game data into RomForge", kind: "warning" },
    );
    if (!ok) return;
    working = true;
    const id = await run(`Moving ${label}'s game data…`, "move_data", { emulator });
    if (id) note = `${label}'s game data is in RomForge now; the originals are in the trash.`;
    working = false;
    await load();
  }

  async function undo() {
    const ok = await ask(`Put ${label}'s game data back where it was?`, { title: "Undo the move", kind: "warning" });
    if (!ok) return;
    working = true;
    const n = await run(`Putting ${label}'s game data back…`, "undo_move_data", { emulator });
    if (n != null) note = `${label}'s game data is back where it was.`;
    working = false;
    await load();
  }
</script>

{#if plan}
  <section class="setting">
    <span class="flabel">Your game data</span>
    {#if plan.moved}
      <p class="hint">{label}'s game data is in RomForge; romforge's setup of it uses that copy.</p>
      <div class="pick"><button onclick={undo} disabled={working}>Put it back</button></div>
    {:else if plan.items.length}
      <p class="hint">
        romforge points {label} at your data where it is. You can move it into RomForge instead:
      </p>
      <ul class="items">
        {#each plan.items as i}
          <li><strong>{i.what}</strong> · {i.files} file{i.files === 1 ? "" : "s"}, {size(i.bytes)} · <bdi>{i.from}</bdi> → <bdi>{i.to}</bdi></li>
        {/each}
      </ul>
      {#if plan.problem}<p class="hint">Can't move it now: {plan.problem}.</p>{/if}
      <div class="pick"><button onclick={move} disabled={working || !!plan.problem}>Move into RomForge…</button></div>
    {:else}
      <p class="hint">None of {label}'s game data was found outside RomForge.</p>
    {/if}
  </section>
{/if}

<style>
  .items { margin: 0.2rem 0 0.4rem 1rem; padding: 0; font-size: 0.8rem; }
  .items li { margin: 0.15rem 0; overflow-wrap: anywhere; }
</style>
