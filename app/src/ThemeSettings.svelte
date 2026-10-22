<script>
  import { invoke } from "@tauri-apps/api/core";
  import { theme, loadThemes, pickTheme, themeChoices, withTheme } from "./fullscreen/theme.svelte.js";
  import ThemeStore from "./ThemeStore.svelte";
  let storeOpen = $state(false);

  // Settings → Display: the fullscreen mode's theme and what it offers.
  // Each change is kept at once (as in the fullscreen mode's Start →
  // Theme), not with the dialog's Save. Refused themes are listed with why.
  loadThemes();

  let v = $derived(theme.view);
  let sel = $derived(v?.selection);
  let c = $derived(themeChoices(v));
  let m = $derived(c?.manifest);
  let refused = $derived(v?.themes.filter((t) => t.problem) ?? []);
  const set = (change) => pickTheme({ ...sel, ...change });
</script>

{#if c}
  <section class="setting">
    <span class="flabel">Fullscreen theme</span>
    <p class="hint">How the fullscreen mode looks. Kept as soon as you change it.</p>
    <div class="grid">
      <label for="fs-theme">Theme</label>
      <select id="fs-theme" value={sel.theme} onchange={(e) => set(withTheme(e.currentTarget.value))}>
        {#each c.themes as t}<option value={t.id}>{t.label}</option>{/each}
      </select>
      <label for="fs-scheme">Colours</label>
      <select id="fs-scheme" value={c.scheme} onchange={(e) => set({ scheme: e.currentTarget.value })}>
        {#each c.schemes as s}<option value={s.value}>{s.label}</option>{/each}
      </select>
      {#if c.layouts.length > 1}
        <label for="fs-layout">Layout</label>
        <select id="fs-layout" value={c.layout} onchange={(e) => set({ layout: e.currentTarget.value })}>
          {#each c.layouts as l}<option value={l.value}>{l.label}</option>{/each}
        </select>
      {/if}
      <label for="fs-size">Text size</label>
      <select id="fs-size" value={c.textSize} onchange={(e) => set({ text_size: Number(e.currentTarget.value) })}>
        {#each c.textSizes as n}<option value={n}>{n}%</option>{/each}
      </select>
      {#each c.options as o}
        <label for="fs-opt-{o.key}">{o.label}</label>
        <select
          id="fs-opt-{o.key}"
          value={o.value}
          onchange={(e) => set(c.withOption(o.key, e.currentTarget.value))}
        >
          {#each o.choices as c}<option value={c.value}>{c.label}</option>{/each}
        </select>
      {/each}
    </div>
    {#if m.description}<p class="hint">{m.description}{m.author ? ` By ${m.author}` : ""}{m.licence ? ` (${m.licence}).` : ""}</p>{/if}
    <p><button onclick={() => (storeOpen = true)}>Get themes…</button></p>
    {#if v.dir}
      <p class="hint">
        Your own themes go in <code>{v.dir}</code>, a folder each.
        <button onclick={() => invoke("themes_folder").catch(() => {})}>Open the folder</button>
      </p>
    {/if}
    {#each refused as t}
      <p class="hint bad">{t.id}: {t.problem}</p>
    {/each}
  </section>
{/if}
<ThemeStore bind:open={storeOpen} />

<style>
  .grid { display: grid; grid-template-columns: auto minmax(10rem, 18rem); gap: 0.35rem 0.8rem; align-items: center; margin: 0.3rem 0; }
  .bad { color: var(--bad); }
</style>
