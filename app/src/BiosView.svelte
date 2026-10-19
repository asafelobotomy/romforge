<script>
  import { invoke } from "@tauri-apps/api/core";

  // The BIOS page (milestone 20): what each system's emulators load, from
  // RetroBIOS's list, against the files in the RomForge folder's Bios/.
  // Always mounted, so the system picked lasts; drawn only while `active`.
  let { active, busy, note = $bindable(""), error = $bindable(""), run } = $props();

  let report = $state(null);
  let problem = $state("");
  let key = $state(null);
  let showAll = $state(false);

  export async function loadBios() {
    problem = "";
    try {
      report = await invoke("firmware_report");
      if (!report.systems.some((s) => s.key === key)) key = report.systems[0]?.key ?? null;
    } catch (e) {
      report = null;
      problem = String(e).replace(/^(launch|settings): /, "");
    }
  }

  async function download() {
    const n = await run("Fetching RetroBIOS's list…", "firmware_download", {});
    if (n) note = `RetroBIOS's list: ${n} emulators and cores. Only the list is fetched, never BIOS files.`;
    await loadBios();
  }

  // An emulator's state in a few words.
  function readiness(e) {
    if (!e.profiled) return "not in RetroBIOS's list";
    if (!e.ready) return "missing required files";
    return e.regions.length ? `ready for ${e.regions.join(", ")}` : "ready";
  }
  const STATES = { accepted: "Accepted", present: "Present (no hash to check)", wrong_hash: "Wrong file", missing: "Missing" };
  const need = (f) => (f.required ? (f.hle ? "Required, runs without" : "Required") : "Optional");
  // Where a file goes, from Bios/ on.
  const where = (f) => (report && f.place.startsWith(report.bios_dir) ? "Bios" + f.place.slice(report.bios_dir.length) : f.place);
  const readyCount = (s) => s.emulators.filter((e) => e.ready && e.profiled).length;
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
      only, never BIOS files: put your own dumps where each one goes. A file is accepted when its size and hash are ones
      the emulator takes.
    </p>
    {#if problem}
      <p class="hint" role="status">{problem}</p>
    {:else if report}
      <div class="bios">
        <div class="syspane">
          <table>
            <thead><tr><th>Platform</th><th class="num">Ready</th></tr></thead>
            <tbody>
              {#each report.systems as s}
                <tr class:selected={s.key === key} onclick={() => (key = s.key)}>
                  <td>{s.name}</td>
                  <td class="num" class:none={readyCount(s) === 0}>{readyCount(s)} / {s.emulators.length}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="emupane">
          {#if system}
            <label class="toggle"><input type="checkbox" bind:checked={showAll} /> Show optional files you don't have</label>
            {#each system.emulators as e}
              {@const files = e.files.filter((f) => showAll || f.required || f.state !== "missing")}
              <section class="emu">
                <h3>{e.label}</h3>
                <p class="hint">
                  <span class:good={e.ready && e.profiled} class:bad={!e.ready}>{readiness(e)}</span>
                  · {e.installed ? "installed" : "not installed"} · {e.files.length} file{e.files.length === 1 ? "" : "s"} in its list
                </p>
                {#if files.length}
                  <table>
                    <thead><tr><th>File</th><th>State</th><th>Needed</th><th>Goes in</th></tr></thead>
                    <tbody>
                      {#each files as f}
                        <tr>
                          <td>{f.name}{#if f.regions.length}<span class="hint"> · {f.regions.join(", ")}</span>{/if}</td>
                          <td class={f.state}>{STATES[f.state]}</td>
                          <td>{need(f)}</td>
                          <td class="path" title={f.place}>{where(f)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                {:else if e.profiled}
                  <p class="hint">{e.files.length ? "Only optional files, none here yet." : "Needs no BIOS files."}</p>
                {/if}
              </section>
            {/each}
          {/if}
        </div>
      </div>
    {/if}
  </section>
{/if}

<style>
  .bios { display: grid; grid-template-columns: minmax(12rem, 0.6fr) minmax(0, 2fr); gap: 1rem; align-items: start; }
  .syspane, .emupane { overflow-x: auto; min-width: 0; }
  .syspane tbody tr { cursor: pointer; }
  tr.selected { background: var(--accent-bg); }
  .emu { border-top: 1px solid var(--line); padding: 0.5rem 0; }
  .emu h3 { font-size: 0.95rem; margin: 0; }
  .emu .hint { margin: 0.15rem 0 0.35rem; }
  .good, .accepted { color: var(--good, #3a3); }
  .bad, .wrong_hash { color: var(--bad); }
  .missing, .none { color: var(--muted); }
  .path { font-size: 0.8rem; word-break: break-all; }
</style>
