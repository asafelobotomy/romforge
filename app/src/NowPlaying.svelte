<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { artSrc } from "./lib.js";
  import Splash from "./Splash.svelte";

  // A game while it runs (milestone 18, 1.6): a fullscreen splash with its
  // art from Play until the emulator's window takes over (it takes the
  // focus), then a bar with the game, how long it has run, and Stop, which
  // asks the emulator to quit first (it saves) and stops it only if it
  // doesn't. `ended` is called when the game ends (its play time is kept).
  let { playing = $bindable(null), note = $bindable(""), error = $bindable(""), ended } = $props();

  const win = getCurrentWindow();
  let splash = $state(null);
  let now = $state(Date.now());
  let stopping = $state(false);
  let fallback = null;

  /// Show the splash for `p` (what `launch` answered), with the game's
  /// `art` ({ box3d, boxart, platform }) when Details has it.
  export async function started(p, art) {
    playing = p;
    splash = { title: p.game, emulator: p.emulator, logo: null, ...art };
    invoke("platform_logo", { system: p.system })
      .then((file) => splash && file && (splash.logo = artSrc(file)))
      .catch(() => {});
    clearTimeout(fallback);
    fallback = setTimeout(() => hide(true), 15000);
    try {
      await win.setFullscreen(true);
    } catch {
      // Not allowed or not supported: the splash still covers the window.
    }
  }

  // `away`: the game is up, so the window goes out of its way as Settings
  // says (to the tray, minimised, or left); it comes back when the game
  // ends (the backend shows it again).
  async function hide(away = false) {
    if (!splash) return;
    splash = null;
    clearTimeout(fallback);
    try {
      await win.setFullscreen(false);
    } catch {
      // As above.
    }
    if (away && playing) invoke("tuck_away", { game: playing.game }).catch(() => {});
  }

  const minutes = (s) => (s < 90 ? `${s} s` : s < 5400 ? `${Math.round(s / 60)} min` : `${(s / 3600).toFixed(1)} h`);

  // The emulator's window taking the focus means it's up.
  $effect(() => {
    let off = null;
    win.onFocusChanged(({ payload: focused }) => !focused && hide(true)).then((f) => (off = f));
    return () => off?.();
  });

  $effect(() => {
    let off = null;
    listen("game-ended", (e) => {
      const d = e.payload;
      hide();
      playing = null;
      if (d.seconds < 10 && d.code !== 0 && d.code !== null) {
        error = `${d.playing.emulator} closed straight away (exit code ${d.code}); check its setup in Emulator Settings.`;
      } else {
        note = `${d.playing.game}: played ${minutes(d.seconds)}.`;
      }
      ended?.();
    }).then((f) => (off = f));
    return () => off?.();
  });

  // A game still running from before the page reloaded.
  invoke("now_playing").then((p) => p && (playing = p));

  $effect(() => {
    if (!playing) return;
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  async function stop() {
    stopping = true;
    try {
      if (await invoke("stop_game")) note = "It didn't quit when asked, so it was stopped.";
    } catch (e) {
      error = String(e);
    }
    stopping = false;
  }
</script>

{#if splash}<Splash {...splash} />{/if}

{#if playing}
  <div class="playing" role="status">
    <span class="dot" aria-hidden="true"></span>
    <span class="what">Playing <strong>{playing.game}</strong> · {playing.emulator} · {minutes(Math.max(0, Math.floor(now / 1000) - playing.started))}</span>
    <button onclick={stop} disabled={stopping} title="Asks the emulator to quit (it saves first); stops it only if it hasn't after 10 seconds">{stopping ? "Stopping…" : "Stop"}</button>
  </div>
{/if}

<style>
  .playing { display: flex; align-items: center; gap: 0.6rem; padding: 0.25rem 0.75rem; border-bottom: 1px solid var(--line); background: var(--accent-bg); font-size: 0.85rem; }
  .what { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dot { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--ok); }
</style>
