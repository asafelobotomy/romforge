<script>
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { run } from "./jobs.svelte.js";
  import { loadThemes } from "./fullscreen/theme.svelte.js";

  // Settings → Display → Get themes…: the community index (romforge-themes),
  // each theme with its preview, author, version and licence. Nothing is
  // downloaded until its licence is shown and accepted; updates are offered
  // when the index has a newer version. Installs are jobs (footer).
  let { open = $bindable(false) } = $props();
  let list = $state(null);
  let error = $state("");
  let asking = $state(null);
  let previews = $state({});

  $effect(() => {
    if (open) refresh();
  });

  async function refresh() {
    error = "";
    try {
      list = await invoke("themes_available");
      for (const t of list.themes) {
        if (t.preview && !(t.id in previews)) {
          previews[t.id] = null;
          invoke("theme_preview", { id: t.id })
            .then((src) => (previews[t.id] = src))
            .catch(() => {});
        }
      }
    } catch (e) {
      list = null;
      error = `The theme list couldn't be fetched (${String(e)}). Your themes still work.`;
    }
  }

  async function install(t) {
    asking = null;
    const verb = t.status.state === "update_available" ? "Updating" : "Getting";
    const done = await run(`${verb} the ${t.name} theme…`, "theme_install", { id: t.id });
    if (done !== null) {
      await loadThemes();
      await refresh();
    }
  }

  async function remove(t) {
    try {
      await invoke("theme_remove", { id: t.id });
      await loadThemes();
      await refresh();
    } catch (e) {
      error = String(e).replace(/^settings: /, "");
    }
  }
</script>

{#if open}
  <div class="scrim store" role="presentation" onclick={(e) => e.target === e.currentTarget && (open = false)}>
    <div class="dialog wide-dialog" role="dialog" aria-modal="true" aria-labelledby="store-title">
      <h2 id="store-title">Get themes</h2>
      <p class="hint">Themes made by the community, from <a href="https://github.com/asafelobotomy/romforge-themes" onclick={(e) => (e.preventDefault(), openUrl("https://github.com/asafelobotomy/romforge-themes"))}>romforge-themes</a>. Each is checked before it goes in, and keeps its own licence.</p>
      {#if error}<p class="hint bad">{error}</p>{/if}
      {#if list && !list.themes.length}<p class="hint">No themes in the list yet.</p>{/if}
      {#each list?.themes ?? [] as t (t.id)}
        <article class="theme">
          <div class="pic">
            {#if previews[t.id]}<img src={previews[t.id]} alt="" />{:else}<span>{t.name}</span>{/if}
          </div>
          <div class="about">
            <h3>{t.name} <span class="ver">{t.version}</span></h3>
            <p>{t.description}</p>
            <p class="hint">
              By {t.author || "unknown"} · licence
              {#if t.licence_url}<a href={t.licence_url} onclick={(e) => (e.preventDefault(), openUrl(t.licence_url))}>{t.licence}</a>{:else}{t.licence}{/if}
              · {Math.ceil(t.files.reduce((n, f) => n + f.size, 0) / 1024)} KB
            </p>
            {#if asking === t.id}
              <p class="licence">
                <strong>{t.name}</strong> by {t.author || "unknown"} is under <strong>{t.licence}</strong>.
                {#if t.licence_url}Read it before you download.{/if}
              </p>
              <div class="bar">
                <button class="primary" onclick={() => install(t)}>Accept and download</button>
                <button onclick={() => (asking = null)}>Cancel</button>
              </div>
            {:else}
              <div class="bar">
                {#if t.status.state === "not_installed"}
                  <button onclick={() => (asking = t.id)}>Get</button>
                {:else if t.status.state === "update_available"}
                  <button onclick={() => (asking = t.id)}>Update ({t.status.installed} → {t.version})</button>
                  <button onclick={() => remove(t)}>Remove</button>
                {:else if t.status.state === "installed"}
                  <span class="hint">Installed</span>
                  <button onclick={() => remove(t)}>Remove</button>
                {:else if t.status.state === "taken"}
                  <span class="hint">A theme of yours (or a built-in one) already has this name.</span>
                {:else}
                  <span class="hint bad">Can't be installed: {t.status.problem}</span>
                {/if}
              </div>
            {/if}
          </div>
        </article>
      {/each}
      <div class="bar end"><button onclick={() => (open = false)}>Close</button></div>
    </div>
  </div>
{/if}

<style>
  .store { z-index: 60; }
  .theme { display: grid; grid-template-columns: 12rem 1fr; gap: 1rem; padding: 0.8rem 0; border-top: 1px solid var(--line); }
  .pic { display: grid; place-items: center; aspect-ratio: 16 / 9; background: var(--bg); border-radius: 6px; overflow: hidden; color: var(--muted); }
  .pic img { width: 100%; height: 100%; object-fit: cover; }
  h3 { margin: 0 0 0.3rem; font-size: 1rem; color: var(--text); }
  .ver { color: var(--muted); font-weight: 400; font-size: 0.85rem; }
  .about p { margin: 0 0 0.35rem; }
  .licence { padding: 0.5rem 0.7rem; border-radius: 6px; background: var(--accent-bg); }
  .bad { color: var(--bad); }
  .end { justify-content: flex-end; }
  .primary { background: var(--accent); color: #fff; border-color: var(--accent); }
</style>
