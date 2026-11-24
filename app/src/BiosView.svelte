<script>
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import BiosImport from "./BiosImport.svelte";
  import BiosDialog from "./BiosDialog.svelte";
  import { planPlaces, summary } from "./biosCards.js";
  import { artSrc } from "./lib.js";

  // The BIOS page (milestone 20): what each system's emulators load, from
  // RetroBIOS's list, against the files in the RomForge folder's Bios/.
  // Always mounted, so the import plan on screen lasts; drawn only while
  // `active`.
  // `needs`: the library's BIOS badges, kept current from each report.
  // `icons`, `iconStyle`: the sidebar's platform pictures (system key ->
  // file) and their theme, for the cards.
  let { active, busy, note = $bindable(""), error = $bindable(""), run, needs = $bindable([]), icons = {}, iconStyle = "" } = $props();

  let report = $state(null);
  let problem = $state("");
  // The system whose dialog is open.
  let key = $state(null);
  // The import plan on screen, if any: the cards count what it would fill.
  let plan = $state(null);
  let failed = $state(new Set());

  export async function loadBios() {
    problem = "";
    try {
      report = await invoke("firmware_report");
      needs = report.needs;
      if (!report.systems.some((s) => s.key === key)) key = null;
    } catch (e) {
      report = null;
      problem = String(e).replace(/^(launch|settings): /, "");
    }
  }

  // RPCS3 installs PS3 firmware rather than loading a file.
  async function installPs3() {
    const ok = await ask("Run RPCS3's firmware installer on Bios/rpcs3/PS3UPDAT.PUP?", {
      title: "Install PS3 firmware",
      kind: "warning",
    });
    if (!ok) return;
    const where = await run("Installing the PS3 firmware into RPCS3…", "install_ps3_firmware", {});
    if (where) note = `PS3 firmware installed into ${where}.`;
    await loadBios();
  }

  async function download() {
    const n = await run("Fetching RetroBIOS's list…", "firmware_download", {});
    if (n) note = `RetroBIOS's list: ${n} emulators and cores. Only the list is fetched, never BIOS files.`;
    await loadBios();
  }

  const planned = $derived(planPlaces(plan));
  // Each card's numbers, and whether the system can start its games:
  // ready (an emulator is), lacking (none is) or unknown (no list for it).
  const cards = $derived(
    (report?.systems ?? []).map((s) => {
      const n = summary(s, planned);
      const mood = n.ready ? "ready" : s.emulators.some((e) => e.profiled) ? "lacking" : "unknown";
      return { s, n, mood };
    }),
  );
  let system = $derived(report?.systems.find((s) => s.key === key));
</script>

{#if active}
  <section class="wide">
    <div class="bar">
      <button onclick={download} disabled={!!busy}>{report ? "Update BIOS list" : "Download BIOS list"}</button>
      {#if report}<span class="hint">RetroBIOS's list of {report.generated_at.slice(0, 10)} · your files go in {report.bios_dir}</span>{/if}
    </div>
    <p class="hint">
      Which files each emulator loads comes from RetroBIOS, read from the emulators' own source. romforge fetches that list
      only, never BIOS files: bring your own dumps. A file is accepted when its size and hash are ones the emulator takes.
    </p>
    {#if report}<BiosImport {active} {busy} bind:note bind:error bind:plan done={loadBios} order={report.systems.map((s) => s.name)} />{/if}
    {#if problem}
      <p class="hint" role="status">{problem}</p>
    {:else if report}
      <div class="cards">
        {#each cards as { s, n, mood } (s.key)}
          <button class="card {mood}" onclick={() => (key = s.key)} title="{s.name}: every BIOS file, where it is and how it matched">
            {#if icons[s.key] && !failed.has(icons[s.key])}
              <img class="picon" class:logo={iconStyle === "logo"} src={artSrc(icons[s.key])} alt="" onerror={() => (failed = new Set(failed).add(icons[s.key]))} />
            {:else}
              <span class="picon short">{s.key}</span>
            {/if}
            <span class="name">{s.name}</span>
            {#if n.places}
              <span class="hint">{n.files} BIOS file{n.files === 1 ? "" : "s"} · {n.places} place{n.places === 1 ? "" : "s"}</span>
              <span class="count"><strong>{n.have}</strong> / {n.places} matched{#if n.incoming}<span class="incoming">&nbsp;· +{n.incoming} in this import</span>{/if}</span>
              <span class="hint">{n.ready} / {n.emulators} emulator{n.emulators === 1 ? "" : "s"} ready</span>
            {:else}
              <span class="hint">{mood === "unknown" ? "Not in RetroBIOS's list" : "Needs no BIOS files"}</span>
            {/if}
          </button>
        {/each}
      </div>
      {#if system}<BiosDialog {system} {report} {planned} {busy} {installPs3} onclose={() => (key = null)} />{/if}
    {/if}
  </section>
{/if}

<style>
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr)); gap: 0.6rem; }
  .card { display: flex; flex-direction: column; align-items: flex-start; gap: 0.15rem; text-align: left; padding: 0.6rem 0.75rem; border: 1px solid var(--line); border-left: 4px solid var(--line); border-radius: var(--radius); background: var(--panel); cursor: pointer; font: inherit; color: inherit; }
  .card:hover, .card:focus-visible { background: var(--accent-bg); }
  .card.ready { border-left-color: var(--ok); }
  .card.lacking { border-left-color: var(--bad); }
  .picon { width: 3rem; height: 2.4rem; object-fit: contain; margin-bottom: 0.2rem; }
  .picon.logo { box-sizing: border-box; padding: 0.2rem 0.25rem; background: #f4f4f4; border-radius: var(--radius-control); }
  .picon.short { display: grid; place-items: center; width: auto; min-width: 3rem; padding: 0 0.3rem; font-size: 0.75rem; font-weight: 700; border: 1px solid var(--line); border-radius: var(--radius-control); }
  .name { font-weight: 600; }
  .card .hint { margin: 0; font-size: 0.8rem; }
  .incoming { color: var(--accent); }
</style>
