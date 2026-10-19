<script>
  import { VIEWS } from "./listing.js";

  // List, box art, 3D box or banner: the library's and the Versions tab's
  // view. `centered`: on a line of its own.
  let { mode = $bindable(), label, centered = false } = $props();
</script>

<div class="views" class:centered role="group" aria-label={label}>
  {#each VIEWS as [k, name, icon]}
    <button class:active={mode === k} aria-pressed={mode === k} onclick={() => (mode = k)} title="View as {name.toLowerCase()}"><span aria-hidden="true">{icon}</span> {name}</button>
  {/each}
</div>

<style>
  .views { display: inline-flex; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .views button { border: none; border-radius: 0; padding: 0.25rem 0.55rem; font-size: 0.8rem; background: none; color: var(--muted); }
  .views button + button { border-left: 1px solid var(--line); }
  .views button.active { background: var(--accent); color: #fff; }
  .views.centered { display: flex; width: max-content; margin: 0 auto 0.6rem; }
</style>
