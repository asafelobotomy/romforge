<script>
  import { invoke } from "@tauri-apps/api/core";

  // Emulator Settings' Hotkeys tab: romforge's own game pad hotkeys (it
  // reads them itself while a game runs; the emulators' pad hotkeys stay
  // off), the radial menu, and what each installed emulator gets. The
  // on/off switch is the Global tab's "Game pad hotkeys".
  let { installed = [], on = true } = $props();

  let v = $state(null);
  $effect(() => {
    invoke("hotkeys_view", { emulators: installed.map((e) => e.name) })
      .then((r) => (v = r))
      .catch(() => (v = null));
  });
  const labelOf = (name) => installed.find((e) => e.name === name)?.label ?? name;
  const actionLabel = (a) => v?.hotkeys.find((h) => h.action === a)?.label ?? a;
</script>

{#if v}
  <p class="hint">
    romforge's own hotkeys, the same for every emulator: it reads them on the pad while a game runs. Select is the hotkey button.
    {on ? "" : "They are off: turn on Game pad hotkeys in the Global tab."}
  </p>
  <table class="keys">
    <tbody>
      {#each v.hotkeys as h}
        <tr>
          <td class="chord">{#each h.chord as b, i}{#if i} + {/if}<kbd>{b}</kbd>{/each}</td>
          <td>{h.label}</td>
        </tr>
      {/each}
      <tr>
        <td class="chord"><kbd>{v.radial_button}</kbd> held {v.radial_hold_secs} s</td>
        <td>A ring of actions over the game ({v.radial.map((s) => s.label).join(", ")}): the right stick picks, letting go of {v.radial_button} does it.</td>
      </tr>
    </tbody>
  </table>
  {#if v.support.length}
    <h3>What each emulator gets</h3>
    <ul class="support">
      {#each v.support as s}
        <li>
          <strong>{labelOf(s.emulator)}</strong>: {s.actions.length === v.hotkeys.length ? "all of them" : s.actions.map(actionLabel).join(", ")}
          {#if s.note}<span class="hint">{s.note}</span>{/if}
        </li>
      {/each}
    </ul>
  {/if}
{/if}

<style>
  .keys { border-collapse: collapse; margin: 0.4rem 0 0.8rem; }
  .keys td { padding: 0.3rem 0.7rem 0.3rem 0; border-bottom: 1px solid var(--line); vertical-align: top; }
  .chord { white-space: nowrap; }
  kbd { padding: 0.05rem 0.35rem; border: 1px solid var(--line); border-radius: 4px; background: var(--panel); font: inherit; font-size: 0.85rem; font-weight: 600; }
  .support { margin: 0.2rem 0; padding-left: 1.1rem; }
</style>
