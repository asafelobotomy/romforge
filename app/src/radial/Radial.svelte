<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  // The radial menu over a running game (hotkeys.rs): a ring of actions,
  // the one the right stick points at lit, the ones the running emulator
  // can't do dimmed. The pad drives it; this page only shows it.
  let open = $state(false);
  let slot = $state(null);
  let slots = $state([]);
  let can = $state(null);

  async function load() {
    const now = await invoke("now_playing").catch(() => null);
    const v = await invoke("hotkeys_view", { emulators: now ? [now.emulator] : [] }).catch(() => null);
    if (!v) return;
    slots = v.radial;
    can = v.support[0]?.actions ?? null;
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
  <div class="ring" style="--step: {step}deg">
    {#each slots as s, i}
      <div class="slot" class:on={slot === i} class:off={can && !can.includes(s.action)} style="--i: {i}">{s.label}</div>
    {/each}
    <div class="hub">{slot === null ? "Pick with the right stick" : "Let go of L3"}</div>
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
    width: 440px;
    height: 440px;
    border-radius: 50%;
    background: rgb(10 12 20 / 0.78);
    box-shadow: 0 0 0 3px rgb(255 255 255 / 0.25);
  }
  /* Clockwise from the top, as the stick picks them. */
  .slot {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 130px;
    margin: -24px 0 0 -65px;
    padding: 12px 0;
    text-align: center;
    border-radius: 12px;
    transform: rotate(calc(var(--i) * var(--step) - 90deg)) translate(155px) rotate(calc(var(--i) * var(--step) * -1 + 90deg));
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
