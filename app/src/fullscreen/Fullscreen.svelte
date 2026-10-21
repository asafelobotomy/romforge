<script>
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "../lib.js";
  import Wordmark from "../Wordmark.svelte";
  import { fs, leave, remembered, remember } from "./fs.svelte.js";
  import { homeAction, keyAction, listMove } from "./fsnav.js";
  import { ask, contextMenu, mainMenu, optionsMenu } from "./fsmenus.js";
  import FsCarousel from "./FsCarousel.svelte";
  import FsGrid from "./FsGrid.svelte";
  import FsInfo from "./FsInfo.svelte";
  import FsGame from "./FsGame.svelte";
  import FsMenu from "./FsMenu.svelte";
  import FsHelp from "./FsHelp.svelte";
  import "./fullscreen.css";

  // romforge's fullscreen mode (stage 2): the platforms along the top, a
  // platform's games below with the focused one's facts beside them, a
  // game's own page, menus, and a help bar; all of it by pad or keyboard.
  // `play(row, art)` starts a game as the desktop's Play does;
  // `desktop(id)` leaves for the desktop library with that game picked.
  let { lib, icons = {}, iconStyle = "", playing = null, play, desktop } = $props();

  const saved = remembered();
  let sort = $state(saved.sort ?? "title");
  let art = $state(saved.art ?? "boxart");
  let platformKey = $state(saved.platform ?? "*");
  let gameId = $state(saved.game ?? null);
  let focus = $state("grid");
  let screen = $state("home");
  let menu = $state(null);
  let button = $state(0);
  let cols = $state(5);
  let rows = $state(2);
  $effect(() => remember({ sort, art, platform: platformKey, game: gameId }));

  const title = (r) => (r.name?.title || r.label || r.title).toLowerCase();
  let all = $derived(lib.rows.filter((r) => r.main && !r.unmatched && r.system));
  let platforms = $derived([{ system: "*", name: "All games", short: "ALL", games: all.length }, ...lib.platforms.filter((p) => p.system && p.games)]);
  let pIndex = $derived(Math.max(0, platforms.findIndex((p) => p.system === platformKey)));
  let games = $derived(
    all
      .filter((r) => platformKey === "*" || r.system === platformKey)
      .sort((a, b) => (sort === "year" ? (a.year ?? 9999) - (b.year ?? 9999) : 0) || title(a).localeCompare(title(b))),
  );
  let gIndex = $derived(Math.max(0, games.findIndex((r) => r.id === gameId)));
  let game = $derived(games[gIndex] ?? null);
  const versionsOf = (g) => (g?.group ? lib.rows.filter((r) => r.group === g.group && r.id !== g.id && !r.head) : []);

  // The focused game's details (facts, art), a moment after the focus stops.
  let detail = $state(null);
  $effect(() => {
    const id = game?.id;
    if (!id) return;
    const t = setTimeout(async () => {
      const d = await invoke("game_detail", { id }).catch(() => null);
      if (!d || game?.id !== id) return;
      d.art = Object.fromEntries(Object.entries(d.art ?? {}).map(([k, p]) => [k, artSrc(p)]));
      detail = d;
    }, 150);
    return () => clearTimeout(t);
  });

  // What the menus' items do.
  const act = {
    leave,
    quit: () => invoke("quit"),
    sort: (s) => (sort = s),
    art: (a) => (art = a),
    desktop: (g) => desktop(g.id),
    play: (g) => start(g),
  };
  function start(g) {
    if (!g?.playable) return;
    const p = g.pictures ?? {};
    play(g, { box3d: artSrc(p.box3d), boxart: artSrc(p.boxart), platform: g.system_name });
  }

  let buttons = $derived(
    game
      ? [
          ...(game.playable ? [{ id: "play", label: "Play", primary: true, run: () => start(game) }] : []),
          ...(versionsOf(game).length ? [{ id: "versions", label: "Versions", run: () => (menu = optionsMenu(act, game, versionsOf(game))) }] : []),
          { id: "back", label: "Back", run: () => (screen = "home") },
        ]
      : [],
  );

  function pressMenu(i) {
    const next = menu?.items[i]?.run();
    menu = next ?? null;
  }

  // One action from the pad or the keyboard.
  function handle(action) {
    if (playing) return;
    if (menu) {
      if (action === "back") menu = null;
      else if (action === "accept") pressMenu(menu.index);
      else menu.index = listMove(menu.index, menu.items.length, action);
      return;
    }
    if (action === "menu") return void (menu = mainMenu(act, { sort, art }));
    if (screen === "game") {
      if (action === "back") screen = "home";
      else if (action === "accept") buttons[button]?.run();
      else if (action === "north") menu = optionsMenu(act, game, versionsOf(game));
      else if (action === "context") menu = contextMenu(act, game, versionsOf(game));
      else if (action === "left" || action === "right") {
        const i = gIndex + (action === "left" ? -1 : 1);
        if (i >= 0 && i < games.length) gameId = games[i].id;
        button = 0;
      } else button = listMove(button, buttons.length, action === "up" ? "left" : action === "down" ? "right" : action);
      return;
    }
    const r = homeAction({ focus, platform: pIndex, platforms: platforms.length, game: gIndex, count: games.length, cols, rows }, action);
    if (r.platform !== undefined) {
      platformKey = platforms[r.platform].system;
      gameId = null;
    }
    if (r.game !== undefined) gameId = games[r.game]?.id ?? null;
    if (r.focus) focus = r.focus;
    if (r.intent === "open") {
      screen = "game";
      button = 0;
    } else if (r.intent === "options") menu = optionsMenu(act, game, versionsOf(game));
    else if (r.intent === "context") menu = contextMenu(act, focus === "grid" ? game : null, focus === "grid" ? versionsOf(game) : []);
    else if (r.intent === "leave") menu = ask("Back to desktop?", leave);
  }

  $effect(() => {
    fs.pad = handle;
    return () => (fs.pad = null);
  });

  let help = $derived(
    menu
      ? [["move", "Move"], ["accept", "Choose"], ["back", "Close"]]
      : screen === "game"
        ? [["accept", buttons[button]?.label ?? "Choose"], ["side", "Game"], ["north", "Options"], ["back", "Back"], ["menu", "Menu"], ["Select+Start", "Quits a game"]]
        : focus === "carousel"
          ? [["side", "Platform"], ["accept", "Games"], ["back", "Desktop"], ["menu", "Menu"]]
          : [["accept", "Open"], ["north", "Options"], ["context", "More"], ["page", "Page"], ["back", "Platforms"], ["menu", "Menu"]],
  );
</script>

<svelte:window
  onkeydown={(e) => {
    const a = keyAction(e.key);
    if (!a) return;
    e.preventDefault();
    handle(a);
  }}
/>

<div class="fs" data-fs-root data-fs-screen={screen}>
  {#if screen === "home"}
    <header class="top"><Wordmark height="3.2vh" /></header>
    <FsCarousel {platforms} index={pIndex} focused={focus === "carousel" && !menu} {icons} {iconStyle} pick={(i) => ((platformKey = platforms[i].system), (gameId = null), (focus = "carousel"))} />
    <main class="home">
      <FsGrid {games} index={gIndex} focused={focus === "grid" && !menu} {art} bind:cols bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      <FsInfo {game} {detail} />
    </main>
  {:else if game}
    <FsGame {game} {detail} {buttons} focus={button} press={(i) => buttons[i]?.run()} />
  {/if}
  <FsHelp items={help} pad={fs.padSeen} />
  {#if menu}<FsMenu {menu} press={pressMenu} />{/if}
</div>
