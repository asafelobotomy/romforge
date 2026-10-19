<script>
  import { invoke } from "@tauri-apps/api/core";
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import Wordmark from "./Wordmark.svelte";

  // First run (milestone 13): where the RomForge folder goes. `root` is
  // set once it exists; `load` reloads the library; `open` while it shows.
  let { open = $bindable(false), root = $bindable(null), note = $bindable(""), load } = $props();

  let parent = $state("");
  let existing = $state(false);
  let creating = $state(false);
  let homeError = $state("");

  export async function check() {
    const info = await invoke("home_info").catch(() => null);
    if (!info) return;
    root = info.root;
    existing = info.existing;
    parent = info.suggested ?? "";
    open = !info.root;
  }

  async function browse() {
    const dir = await pickFolder({ directory: true, multiple: false, defaultPath: parent || undefined });
    if (typeof dir === "string") parent = dir;
  }

  async function create() {
    homeError = "";
    creating = true;
    try {
      root = await invoke("home_create", { parent });
      open = false;
      note = `romforge keeps everything in ${root}` + (existing ? "; your library moved there" : "");
      await load();
    } catch (e) {
      homeError = String(e).replace(/^settings: /, "");
    }
    creating = false;
  }

  const sep = $derived(parent.includes("\\") ? "\\" : "/");
  const target = $derived(parent.replace(/[\\/]+$/, "").endsWith("RomForge") ? parent : `${parent.replace(/[\\/]+$/, "")}${sep}RomForge`);
</script>

{#if open}
  <div class="scrim" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="home-title">
      <div class="hero"><Wordmark height="3.2rem" /></div>
      <h2 id="home-title">Where should romforge keep your games?</h2>
      <p class="hint">
        Everything goes in one folder named RomForge: your games (one file each, named by its DAT), their art, the
        databases you download and the library itself. Move or back up that folder and everything comes with it.
        Moved one by hand? Pick it (or the folder it's in) and romforge picks up where it left off.
      </p>
      <section class="setting">
        <label for="home-parent">Create RomForge in</label>
        <div class="pick">
          <input id="home-parent" bind:value={parent} placeholder="/home/me" />
          <button onclick={browse} disabled={creating}>Browse…</button>
        </div>
        {#if parent}<p class="hint">→ <bdi>{target}</bdi></p>{/if}
      </section>
      {#if existing}
        <p class="hint">Your current library (its database, art and downloaded databases) moves into it. Your ROMs stay where they are until you import them.</p>
      {/if}
      {#if homeError}<p class="err" role="alert">{homeError}</p>{/if}
      <div class="actions">
        <span class="spacer"></span>
        <button class="primary" onclick={create} disabled={!parent.trim() || creating}>
          {creating ? "Creating…" : "Create RomForge folder"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .hero { display: flex; justify-content: center; margin: 0.25rem 0 1rem; }
</style>
