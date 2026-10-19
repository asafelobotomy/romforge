<script>
  import { thumbSrc } from "./lib.js";

  // The library as pictures: box art, 3D boxes or banners (fanart with the
  // game's logo over it), nothing but the picture, in the same order and
  // with the same fold-out versions and headings as the list. With
  // `scroll` (the library) only the lines in view are drawn, so thousands
  // stay quick; without it (Details' Versions) it is as tall as it needs.
  let { rows, mode, selected, isOpen, open, toggle, statusLabel, scroll = true } = $props();

  // Boxes: every box the same height (BOX px) on one baseline, each at its
  // platform's and region's shape (`box_aspect`, width / height in
  // hundredths; a 3D box shows its spine too, so a little wider). Every
  // cell is as wide as the widest box in view. Scans of another shape fit
  // inside whole, never cropped.
  const BOX = 170;
  const SPINE = 1.15;
  // Banners: one shape for all (wide, like SteamGridDB's heroes), the
  // fanart filling it.
  const BANNER = 3.2;
  const HEAD = 34;
  const GAP = 14;

  let width = $state(600);
  let height = $state(600);
  let scrollTop = $state(0);

  const aspect = (r) => ((r.box_aspect || 72) / 100) * (mode === "box3d" ? SPINE : 1);
  const widest = $derived(Math.min(1.6, Math.max(0.55, ...rows.filter((r) => !r.head).map(aspect))));
  const size = $derived(
    mode === "banner"
      ? { w: 0, h: Math.round(Math.min(220, Math.max(90, (width - 2 * GAP) / BANNER))) }
      : { w: Math.round(BOX * widest), h: BOX },
  );
  // How tall a picture is drawn, so only that much is loaded (a thumbnail):
  // a banner's fanart covers the whole row's width, so it is as tall as a
  // 16:9 picture that wide.
  const drawn = $derived(mode === "banner" ? ((width - 2 * GAP) * 9) / 16 : BOX);
  const cols = $derived(size.w ? Math.max(1, Math.floor((width - GAP) / (size.w + GAP))) : 1);

  // Lines: a heading alone, or up to `cols` pictures; each with its offset.
  const lines = $derived.by(() => {
    const out = [];
    let top = 0;
    let cards = null;
    for (const r of rows) {
      if (r.head) {
        cards = null;
        out.push({ head: r, key: r.id, top, h: HEAD });
        top += HEAD;
      } else {
        if (!cards || cards.items.length >= cols) {
          cards = { items: [], key: `l:${r.id}`, top, h: size.h + GAP };
          out.push(cards);
          top += size.h + GAP;
        }
        cards.items.push(r);
      }
    }
    return { lines: out, total: top + GAP };
  });
  const visible = $derived(
    scroll ? lines.lines.filter((l) => l.top + l.h >= scrollTop - 400 && l.top <= scrollTop + height + 400) : lines.lines,
  );

  function picture(r) {
    const p = r.pictures ?? {};
    if (mode === "box3d") return p.box3d ?? p.boxart;
    if (mode === "banner") return p.hero;
    return p.boxart;
  }
  const tip = (r) =>
    [r.label ?? r.title, r.system_name, r.year, statusLabel[r.status]].filter(Boolean).join(" · ");
  // Only what needs saying is marked: a game you don't have, a patch file,
  // a file nothing identified.
  const MARKS = { missing: "○", patch: "✚", unknown: "?", candidate: "~", wrong_system: "~", incomplete: "~" };
  const broken = (e) => e.currentTarget.classList.add("broken");
</script>

<div
  class="grid {mode}"
  class:scroll
  bind:clientWidth={width}
  bind:clientHeight={height}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
>
  <div class="space" style="height: {lines.total}px">
    {#each visible as l (l.key)}
      {#if l.head}
        <button class="head" style="top: {l.top}px; padding-left: {0.6 + l.head.depth * 1.1}rem" onclick={() => toggle(l.head.key)} aria-expanded={isOpen(l.head.key)}>
          {isOpen(l.head.key) ? "▾" : "▸"} {l.head.label}<span class="n"> · {l.head.n}</span>
        </button>
      {:else}
        <div class="line" style="top: {l.top + GAP}px; height: {size.h}px; gap: {GAP}px; padding: 0 {GAP}px; grid-template-columns: repeat({cols}, {size.w ? `${size.w}px` : "1fr"})">
          {#each l.items as r (r.id)}
            {@const src = thumbSrc(picture(r), drawn)}
            {@const logo = mode === "banner" ? thumbSrc(r.pictures?.logo, size.h * 0.7) : null}
            <div class="cell">
              <button
                class="pic"
                class:sel={r.id === selected}
                class:missing={r.status === "missing"}
                style={mode === "banner" ? "" : `height: ${BOX}px; width: ${Math.round(BOX * aspect(r))}px`}
                onclick={() => open(r.id)}
                title={tip(r)}
                aria-label={tip(r)}
              >
                <span class="placeholder">{r.label ?? r.title}</span>
                {#if src}<img class="art" {src} alt="" loading="lazy" decoding="async" onerror={broken} />{/if}
                {#if logo}<img class="logo" src={logo} alt="" loading="lazy" onerror={broken} />{/if}
                {#if MARKS[r.status]}<span class="mark" aria-hidden="true">{MARKS[r.status]}</span>{/if}
              </button>
              {#if r.main && r.versions > 1}
                <button class="more" onclick={() => toggle(r.group)} aria-expanded={isOpen(r.group)} title="{r.versions - 1} more: other regions, editions, translations, hacks and patches">
                  {isOpen(r.group) ? "▾" : "▸"} {r.versions - 1}
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .grid { min-height: 0; }
  .grid.scroll { flex: 1; overflow-y: auto; }
  .space { position: relative; }
  .head { position: absolute; left: 0; right: 0; height: 34px; border: none; border-radius: 0; background: none; text-align: left; color: var(--muted); font-size: 0.85rem; font-weight: 600; border-bottom: 1px solid var(--line); }
  .head .n { font-weight: 400; }
  .line { position: absolute; left: 0; right: 0; display: grid; box-sizing: border-box; justify-content: start; }
  .cell { position: relative; min-width: 0; display: flex; align-items: flex-end; justify-content: center; }

  /* Nothing but the picture: no card behind it, no title under it. */
  .pic { position: relative; display: block; padding: 0; border: none; border-radius: 3px; background: none; cursor: pointer; overflow: hidden; box-shadow: 0 2px 8px #0005; transition: transform 0.12s, box-shadow 0.12s; }
  .pic:hover { transform: translateY(-2px); box-shadow: 0 6px 14px #0006; }
  .pic.sel { outline: 3px solid var(--accent); outline-offset: 2px; }
  .pic.missing { opacity: 0.5; }
  .placeholder { position: absolute; inset: 0; display: grid; place-items: center; padding: 0.5rem; font-size: 0.75rem; line-height: 1.25; color: var(--muted); text-align: center; background: repeating-linear-gradient(45deg, #0001 0 10px, #0002 10px 20px); overflow: hidden; }
  .art { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; }
  .art:global(.broken), .logo:global(.broken) { display: none; }
  /* A 3D render has its own shadow and see-through edges. */
  .box3d .pic { box-shadow: none; }
  .box3d .pic:hover { box-shadow: none; }
  .mark { z-index: 2; position: absolute; top: 4px; right: 4px; min-width: 1.2rem; height: 1.2rem; border-radius: 50%; background: #000b; color: #fff; font-size: 0.7rem; line-height: 1.2rem; text-align: center; }
  .more { position: absolute; top: 4px; left: 4px; padding: 0 0.4rem; height: 1.25rem; border: none; border-radius: 4px; background: #000b; color: #fff; font-size: 0.72rem; z-index: 1; }

  /* Banners: every one the same wide shape, the fanart filling it, the
     game's logo over its left side. */
  .banner .cell { display: block; }
  .banner .pic { width: 100%; height: 100%; border-radius: 6px; }
  .banner .art { object-fit: cover; }
  .banner .pic:has(.logo:not(:global(.broken)))::after { content: ""; position: absolute; inset: 0; background: linear-gradient(90deg, #000a 0%, #0004 40%, #0000 70%); }
  .banner .logo { position: absolute; left: 1rem; top: 50%; transform: translateY(-50%); max-height: 70%; max-width: 40%; z-index: 1; filter: drop-shadow(0 2px 6px #000a); }
  .banner .more { top: 8px; left: 8px; }
</style>
