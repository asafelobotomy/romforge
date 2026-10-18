<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { size } from "./lib.js";

  // Always mounted, so the DAT picked and the filter last until the app
  // closes; drawn only while `active`. `call` and `run` are the app's invoke
  // wrappers (errors and progress on the status line); `load` reloads the
  // library.
  let { active, busy, note = $bindable(""), error = $bindable(""), call, run, load } = $props();

  let summary = $state({ files_by_status: [], dats: [] });
  let datId = $state(null);
  let datGames = $state([]);
  let gameFilter = $state("all");

  // Third-party databases the user downloads (milestone 11).
  let extdbs = $state([]);
  let extBusy = $state("");

  async function loadExtdbs() {
    extdbs = (await call("extdb_list")) ?? [];
  }

  async function getExtdb(d) {
    extBusy = d.db;
    note = `Downloading ${d.name}… (runs in the background; the rest of romforge stays usable)`;
    let failed = "";
    try {
      const r = await invoke("extdb_download", { name: d.db });
      note = `${r.name}: ${r.entries?.toLocaleString()} entries. Enrich now uses it to fill what libretro and Wikidata leave empty.`;
    } catch (e) {
      failed = `${d.name}: ${e}`;
    }
    extBusy = "";
    await loadExtdbs();
    // Set after the reload, which clears errors as it starts.
    if (failed) error = failed;
  }

  async function dropExtdb(d) {
    await call("extdb_remove", { name: d.db });
    note = `${d.name} removed. What Enrich already filled from it stays until you enrich with refresh.`;
    await loadExtdbs();
  }

  export async function loadDats() {
    loadExtdbs();
    summary = (await call("summary")) ?? { files_by_status: [], dats: [] };
    if (datId !== null) await openDat(datId);
  }

  async function openDat(id) {
    datId = id;
    datGames = (await call("dat_games", { datId: id })) ?? [];
  }

  async function downloadDats() {
    const r = await run("Downloading DATs… (a few minutes the first time)", "download_dats", { extras: false });
    if (r) note = `DATs: ${r.imported} imported, ${r.unchanged} unchanged` + (r.errors.length ? `; errors: ${r.errors.join("; ")}` : "");
    await loadDats();
    await load();
  }

  let visibleGames = $derived(datGames.filter((g) => gameFilter === "all" || (gameFilter === "have") === g.have));
  let currentDat = $derived(summary.dats.find((d) => d.dat_id === datId));
</script>

{#if active}
  <section class="wide">
    <div class="bar"><button onclick={downloadDats} disabled={!!busy}>Download DATs for all systems</button></div>
    <section class="extdbs" aria-labelledby="extdb-title">
      <h3 id="extdb-title">Game databases <span class="hint">(optional, downloaded by you)</span></h3>
      <p class="hint">
        Free to download, but published without a licence, so romforge doesn't include them: you fetch them from their
        makers onto this computer. Enrich then uses them only to fill what libretro and Wikidata leave empty.
      </p>
      <ul>
        {#each extdbs as d}
          <li>
            <div class="ehead">
              <a href={d.homepage} onclick={(e) => { e.preventDefault(); openUrl(d.homepage).catch((err) => (error = String(err))); }}>{d.name}</a>
              <span class="hint">
                {d.downloaded_at
                  ? `downloaded ${d.downloaded_at.slice(0, 10)} · ${d.entries?.toLocaleString()} entries · ${size(d.bytes)}`
                  : "not downloaded"}
              </span>
              <span class="spacer"></span>
              {#if d.downloaded_at}<button onclick={() => dropExtdb(d)} disabled={!!extBusy}>Remove</button>{/if}
              <button onclick={() => getExtdb(d)} disabled={!!extBusy}>
                {extBusy === d.db ? "Downloading…" : d.downloaded_at ? "Update" : "Download"}
              </button>
            </div>
            <p class="hint">{d.about}</p>
          </li>
        {/each}
      </ul>
    </section>
    <div class="dats">
      <div class="datpane">
        <table>
          <thead><tr><th>DAT</th><th>System</th><th class="num">Have</th><th class="num">Missing</th></tr></thead>
          <tbody>
            {#each summary.dats as d (d.dat_id)}
              <tr class:selected={d.dat_id === datId} onclick={() => openDat(d.dat_id)}>
                <td>{d.name}{d.version ? ` · v${d.version}` : ""}</td>
                <td>{d.system ?? "—"}</td>
                <td class="num">{d.have} / {d.games}</td>
                <td class="num">{d.missing}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if currentDat}
        <div class="datpane">
          <h2>{currentDat.name}</h2>
          <p class="meta">From {currentDat.source_path}, imported {currentDat.imported_at}</p>
          <div class="bar">
            {#each ["all", "have", "missing"] as v}
              <label><input type="radio" bind:group={gameFilter} value={v} /> {v}</label>
            {/each}
          </div>
          <table>
            <thead><tr><th></th><th>Game</th><th class="num">ROMs</th></tr></thead>
            <tbody>
              {#each visibleGames as g (g.game_id)}
                <tr>
                  <td><span class="badge {g.have ? 'verified' : 'unknown'}">{g.have ? "have" : "missing"}</span></td>
                  <td>{g.name}</td>
                  <td class="num">{g.roms_have}/{g.roms_total}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  </section>
{/if}

<style>
  .extdbs { border: 1px solid var(--line); border-radius: 6px; padding: 0.5rem 0.75rem; margin-bottom: 0.75rem; }
  .extdbs h3 { font-size: 0.95rem; margin: 0 0 0.2rem; }
  .extdbs ul { list-style: none; padding: 0; margin: 0.4rem 0 0; }
  .extdbs li { padding: 0.4rem 0; border-top: 1px solid var(--line); }
  .extdbs .ehead { display: flex; gap: 0.6rem; align-items: baseline; flex-wrap: wrap; }
  .extdbs .ehead a { color: var(--accent); font-weight: 600; }
  .extdbs li .hint { margin: 0.15rem 0 0; }
  .dats { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 1rem; align-items: start; }
  .datpane { overflow-x: auto; min-width: 0; }
  tr.selected { background: var(--accent-bg); }
  .datpane tbody tr { cursor: pointer; }
</style>
