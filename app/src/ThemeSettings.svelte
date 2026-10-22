<script>
  import { invoke } from "@tauri-apps/api/core";
  import { theme, loadThemes, pickTheme } from "./fullscreen/theme.svelte.js";
  import ThemeStore from "./ThemeStore.svelte";
  let storeOpen = $state(false);

  // Settings → Display: the fullscreen mode's theme and what it offers.
  // Each change is kept at once (as in the fullscreen mode's Start →
  // Theme), not with the dialog's Save. Refused themes are listed with why.
  loadThemes();

  let v = $derived(theme.view);
  let sel = $derived(v?.selection);
  let m = $derived(v?.themes.find((t) => t.id === sel?.theme && t.manifest)?.manifest);
  let refused = $derived(v?.themes.filter((t) => t.problem) ?? []);
  const nice = (s) => s.charAt(0).toUpperCase() + s.slice(1).replaceAll("-", " ");
  const set = (change) => pickTheme({ ...sel, ...change });
  let schemes = $derived(m ? Object.keys(m.schemes) : []);
  let follows = $derived(schemes.includes("light") && schemes.includes("dark"));
  let layouts = $derived(m ? [m.layout, ...(m.layouts ?? []).filter((l) => l !== m.layout)] : []);
</script>

{#if v && sel && m}
  <section class="setting">
    <span class="flabel">Fullscreen theme</span>
    <p class="hint">How the fullscreen mode looks. Kept as soon as you change it.</p>
    <div class="grid">
      <label for="fs-theme">Theme</label>
      <select id="fs-theme" value={sel.theme} onchange={(e) => set({ theme: e.currentTarget.value, scheme: "", layout: "" })}>
        {#each v.themes.filter((t) => t.manifest && !t.problem) as t}
          <option value={t.id}>{t.manifest.name}{t.builtin ? "" : t.downloaded ? " (downloaded)" : " (yours)"}</option>
        {/each}
      </select>
      <label for="fs-scheme">Colours</label>
      <select id="fs-scheme" value={sel.scheme || (follows ? "" : schemes[0])} onchange={(e) => set({ scheme: e.currentTarget.value })}>
        {#if follows}<option value="">With the system</option>{/if}
        {#each schemes as s}<option value={s}>{nice(s)}</option>{/each}
      </select>
      {#if layouts.length > 1}
        <label for="fs-layout">Layout</label>
        <select id="fs-layout" value={sel.layout || m.layout} onchange={(e) => set({ layout: e.currentTarget.value })}>
          {#each layouts as l}<option value={l}>{nice(l)}</option>{/each}
        </select>
      {/if}
      <label for="fs-size">Text size</label>
      <select id="fs-size" value={sel.text_size} onchange={(e) => set({ text_size: Number(e.currentTarget.value) })}>
        {#each v.text_sizes as n}<option value={n}>{n}%</option>{/each}
      </select>
      {#each m.options ?? [] as o}
        <label for="fs-opt-{o.key}">{o.label}</label>
        <select
          id="fs-opt-{o.key}"
          value={sel.options?.[sel.theme]?.[o.key] ?? o.choices[0].value}
          onchange={(e) => set({ options: { ...sel.options, [sel.theme]: { ...(sel.options?.[sel.theme] ?? {}), [o.key]: e.currentTarget.value } } })}
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
