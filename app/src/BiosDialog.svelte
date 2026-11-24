<script>
  import { matchedBy } from "./biosCards.js";

  // One platform card's dialog on the BIOS page: every file its emulators
  // load, where it is now, where it goes and how it matched (by its hash or
  // its name), on disk or in the import plan on screen (`planned`: place ->
  // the plan's file for it). `onclose` closes it.
  let { system, report, planned, busy, installPs3, onclose } = $props();

  let showAll = $state(false);

  // An emulator's state in a few words.
  function readiness(e) {
    if (!e.profiled) return "not in RetroBIOS's list";
    if (!e.ready) return "missing required files";
    return e.regions.length ? `ready for ${e.regions.join(", ")}` : "ready";
  }
  const need = (f) =>
    !f.required ? "Optional"
    : f.only_for ? `Only ${f.only_for}`
    : f.hle ? "Required, runs without"
    : "Required";
  const HAVE = new Set(["accepted", "present", "placed_by_you", "incomplete", "wrong_hash"]);
  // Where a file goes, from Bios/ on.
  const where = (p) => (p.startsWith(report.bios_dir) ? "Bios" + p.slice(report.bios_dir.length) : p);
  // Where it is now: at its place, else where the import plan has it.
  const now = (f) => (HAVE.has(f.state) ? where(f.place) : (planned.get(f.place)?.from ?? "—"));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog tall" role="dialog" aria-modal="true" aria-labelledby="bios-title">
    <h2 id="bios-title">{system.name} BIOS files</h2>
    <label class="toggle"><input type="checkbox" bind:checked={showAll} /> Show optional files you don't have</label>
    <div class="body">
      {#each system.emulators as e}
        {@const files = e.files.filter((f) => showAll || f.required || f.state !== "missing" || planned.has(f.place))}
        <section class="emu">
          <h3>{e.label}</h3>
          <p class="hint">
            <span class:good={e.ready && e.profiled} class:bad={!e.ready}>{readiness(e)}</span>
            · {e.installed ? "installed" : "not installed"} · {e.files.length} file{e.files.length === 1 ? "" : "s"} in its list
          </p>
          {#if e.name === "rpcs3" && e.files.some((f) => f.name === "PS3UPDAT.PUP" && f.state !== "missing")}
            <button onclick={installPs3} disabled={!!busy || !e.installed} title="Runs RPCS3's own firmware installer (it goes into the dev_flash romforge's RPCS3 setup uses: your own RPCS3's when it has one)">Install into RPCS3</button>
          {/if}
          {#if files.length}
            <table>
              <thead><tr><th>File</th><th>Needed</th><th>Matched by</th><th>Now at</th><th>Goes in</th></tr></thead>
              <tbody>
                {#each files as f}
                  <tr>
                    <td>{f.name}{#if f.regions.length}<span class="hint">&nbsp;· {f.regions.join(", ")}</span>{/if}</td>
                    <td>{need(f)}</td>
                    <td class={f.state} class:incoming={f.state === "missing" && planned.has(f.place)} title={f.missing?.length ? `Lacks ${f.missing.join(", ")}` : undefined}>
                      {matchedBy(f, planned.get(f.place))}{#if f.missing?.length}<span class="lacks"> · lacks {f.missing.length}</span>{/if}
                    </td>
                    <td class="path" title={now(f)}>{now(f)}</td>
                    <td class="path" title={f.place}>{where(f.place)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else if e.profiled}
            <p class="hint">{e.files.length ? "Only optional files, none here yet." : "Needs no BIOS files."}</p>
          {/if}
        </section>
      {/each}
    </div>
    <div class="actions"><button onclick={onclose}>Close</button></div>
  </div>
</div>

<style>
  .dialog.tall { width: min(68rem, calc(100vw - 2rem)); }
  .emu { border-top: 1px solid var(--line); padding: 0.5rem 0; }
  .emu h3 { font-size: 0.95rem; margin: 0; }
  .emu .hint { margin: 0.15rem 0 0.35rem; }
  table { width: 100%; }
  .good, .accepted { color: var(--ok); }
  .bad, .wrong_hash { color: var(--bad); }
  .missing { color: var(--muted); }
  .incoming { color: var(--accent); }
  .incomplete, .present, .placed_by_you { color: var(--warn, #b8860b); }
  .lacks { font-size: 0.75rem; color: var(--muted); }
  .path { font-size: 0.8rem; word-break: break-all; }
</style>
