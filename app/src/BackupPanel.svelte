<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { run } from "./jobs.svelte.js";

  // "Save backups" on an emulator's tab (milestone 27): its saves that
  // stay outside Saves/ (in its own data) are copied into
  // Saves/<system>/backup/ after each game and when asked, and any can be
  // put back from a copy. The core copies, keeps the last few and
  // journals restores; this only shows and asks.
  let { emulator, label, note = $bindable(""), error = $bindable("") } = $props();

  let saves = $state([]);
  let picked = $state({});
  let working = $state(false);
  // `2026-10-25T15-00-00Z` → a local date and time.
  const when = (v) => new Date(v.replace(/T(\d\d)-(\d\d)-(\d\d)Z$/, "T$1:$2:$3Z")).toLocaleString();

  async function load() {
    saves = await invoke("save_backups", { emulator }).catch((e) => {
      error = String(e);
      return [];
    });
  }
  $effect(() => {
    emulator;
    load();
  });

  async function backup() {
    working = true;
    const d = await run(`Backing up ${label}'s saves…`, "backup_saves", { emulator });
    if (d) note = `${label}: ${d.copied} save${d.copied === 1 ? "" : "s"} copied, ${d.same} unchanged.`;
    working = false;
    await load();
  }

  async function restore(s) {
    const version = picked[s.save] ?? s.versions[0];
    const ok = await ask(
      `Put ${s.save} back as it was on ${when(version)}? The save there now goes to RomForge's trash, so Undo can bring it back.`,
      { title: "Restore a save", kind: "warning" },
    );
    if (!ok) return;
    working = true;
    const id = await run(`Restoring ${label}'s save…`, "restore_save", { emulator, save: s.save, version });
    if (id) note = `${s.save} is back as it was on ${when(version)}.`;
    working = false;
    await load();
  }

  async function undo() {
    working = true;
    const at = await run(`Undoing the last restore…`, "undo_restore_save", { emulator });
    if (at) note = `${at} is as it was before the restore.`;
    working = false;
    await load();
  }
</script>

<section class="setting">
  <span class="flabel">Save backups</span>
  <p class="hint">
    {label} keeps its saves in its own data, so romforge copies each changed one into Saves/ after every game (the last
    five copies of each).
  </p>
  {#if saves.length}
    <ul class="items">
      {#each saves as s}
        <li>
          <bdi>{s.save}</bdi>{#if !s.source}<span class="hint"> (gone from {label})</span>{/if}
          {#if s.versions.length}
            <select bind:value={picked[s.save]} disabled={working}>
              {#each s.versions as v}<option value={v}>{when(v)}</option>{/each}
            </select>
            <button onclick={() => restore(s)} disabled={working}>Restore…</button>
          {:else}
            <span class="hint"> no copy yet</span>
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">No saves of {label}'s found yet.</p>
  {/if}
  <div class="pick">
    <button onclick={backup} disabled={working}>Back up now</button>
    <button onclick={undo} disabled={working}>Undo the last restore</button>
  </div>
</section>

<style>
  .items { margin: 0.2rem 0 0.4rem 1rem; padding: 0; font-size: 0.8rem; max-height: 12rem; overflow-y: auto; }
  .items li { margin: 0.2rem 0; overflow-wrap: anywhere; }
  select { margin-left: 0.4rem; font-size: 0.8rem; }
</style>
