<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { iconFor } from "./icons.js";

  // The radial menu over a running game (hotkeys.rs): a ring of actions,
  // the one the right stick points at lit, the ones the running emulator
  // can't do dimmed. The pad drives it; this page only shows it.
  let open = $state(false);
  let slot = $state(null);
  let slots = $state([]);
  let can = $state(null);
  let button = $state("");
  // A small window of its own, not a layer over the game: its own opaque
  // backdrop (overlay.rs).
  const plain = new URLSearchParams(location.search).has("plain");

  async function load() {
    // The game the app plays, or the one uQ's launcher started (the app is
    // then only this overlay).
    const emulator = await invoke("overlay_emulator").catch(() => null);
    const v = await invoke("hotkeys_view", { emulators: emulator ? [emulator] : [] }).catch(() => null);
    if (!v) return;
    slots = v.radial;
    // What the ring can do in this emulator (its own bindings answer the
    // chords, not the ring).
    can = v.support[0]?.ring ?? null;
    button = v.radial_button;
  }
  load();

  $effect(() => {
    const off = listen("radial", (e) => {
      const ev = e.payload;
      if (ev.kind === "radial_open") {
        open = true;
        slot = null;
        load();
      } else if (ev.kind === "radial_slot") slot = ev.slot;
      else if (ev.kind === "radial_close") open = false;
    });
    return () => off.then((f) => f());
  });

  const step = $derived(360 / Math.max(1, slots.length));
</script>

{#if open && slots.length}
  <div class="ring" class:plain style="--step: {step}deg">
    {#each slots as s, i}
      <div class="slot" class:on={slot === i} class:off={can && !can.includes(s.action)} style="--i: {i}">
        {#if iconFor(s.action)}
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d={iconFor(s.action)} /></svg>
        {/if}
        <span>{s.label}</span>
      </div>
    {/each}
    <div class="hub">{slot === null ? "Pick with the right stick" : `Let go of ${button}`}</div>
  </div>
{/if}

<style>
  :global(html),
  :global(body) {
    margin: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
    font: 600 20px system-ui, sans-serif;
    color: #fff;
  }
  .ring {
    position: absolute;
    inset: 0;
    margin: auto;
    width: 560px;
    height: 560px;
    border-radius: 50%;
    background: rgb(10 12 20 / 0.78);
    box-shadow: 0 0 0 3px rgb(255 255 255 / 0.25);
  }
  .ring.plain {
    width: 100vmin;
    height: 100vmin;
    border-radius: 0;
    box-shadow: none;
    background: rgb(10 12 20);
  }
  /* Clockwise from the top, as the stick picks them. */
  .slot {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 120px;
    margin: -42px 0 0 -60px;
    padding: 10px 0;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    font-size: 17px;
    text-align: center;
    border-radius: 12px;
    transform: rotate(calc(var(--i) * var(--step) - 90deg)) translate(215px) rotate(calc(var(--i) * var(--step) * -1 + 90deg));
  }
  .slot svg {
    width: 34px;
    height: 34px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .slot.on { background: #e8a33a; color: #111; }
  .slot.off { opacity: 0.35; }
  .hub {
    position: absolute;
    inset: 0;
    margin: auto;
    width: 130px;
    height: 130px;
    display: grid;
    place-items: center;
    text-align: center;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.12);
    font-size: 15px;
    padding: 0 10px;
    box-sizing: border-box;
  }
</style>
