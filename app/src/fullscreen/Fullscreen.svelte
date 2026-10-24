<script>
  import { invoke } from "@tauri-apps/api/core";
  import { artSrc } from "../lib.js";
  import Wordmark from "../Wordmark.svelte";
  import { fs, leave, remembered, remember } from "./fs.svelte.js";
  import { homeAction, keyAction, listMove } from "./fsnav.js";
  import { ask, contextMenu, emulatorMenu, mainMenu, optionsMenu, storeMenu } from "./fsmenus.js";
  import { run } from "../jobs.svelte.js";
  import FsCarousel from "./FsCarousel.svelte";
  import FsGrid from "./FsGrid.svelte";
  import FsFlow from "./FsFlow.svelte";
  import FsList from "./FsList.svelte";
  import FsWheel from "./FsWheel.svelte";
  import FsTiles from "./FsTiles.svelte";
  import FsShelves from "./FsShelves.svelte";
  import FsBookshelf from "./FsBookshelf.svelte";
  import FsBox from "./FsBox.svelte";
  import { theme, loadThemes, pickTheme, followTheme } from "./theme.svelte.js";
  import FsInfo from "./FsInfo.svelte";
  import { SHOWN } from "./fsdetails.js";
  import { knowFsInfo } from "./fsinfo.js";
  import { helpBar } from "./fshelp.js";
  import FsGame from "./FsGame.svelte";
  import FsMenu from "./FsMenu.svelte";
  import FsHelp from "./FsHelp.svelte";
  import FsSearch from "./FsSearch.svelte";
  import FsAttract from "./FsAttract.svelte";
  import { idle, poke, setIdle, pauseIdle } from "./attract.svelte.js";
  import { music, startMusic, stopMusic, nextTrack, toggleMusic, setVolume, pauseMusic } from "./music.svelte.js";
  import { gameKey, gamesFor, shelvesFor, virtualPlatforms } from "./fshome.js";
  import { bookcase, SHELF } from "./fsshelf.js";
  import { marks, loadMarks, isFavourite, toggleFavourite } from "./marks.svelte.js";
  import { listen } from "@tauri-apps/api/event";
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

  // The screen's shape, for themes (data-fs-aspect): wide, standard, tall.
  let winW = $state(window.innerWidth);
  let winH = $state(window.innerHeight);
  let aspect = $derived(winW / winH > 1.6 ? "wide" : winW / winH < 1 ? "tall" : "standard");

  // Favourites and play time (step 6): loaded now and after each game.
  loadMarks();
  $effect(() => {
    const off = listen("game-ended", () => loadMarks());
    return () => off.then((f) => f());
  });
  let all = $derived(lib.rows.filter((r) => r.main && !r.unmatched && r.system));
  let platforms = $derived([...virtualPlatforms(all, marks), ...lib.platforms.filter((p) => p.system && p.games)]);
  let pIndex = $derived(Math.max(0, platforms.findIndex((p) => p.system === platformKey)));
  let games = $derived(gamesFor(platforms[pIndex]?.system ?? "*", all, marks, sort));
  // The settings read once on opening (Settings → Display, Start →
  // Details, Start → Bookshelf: the details shown of the focused game and
  // how the bookshelf stands them), and the core's lists (settings_info).
  // A change made here before they answer isn't overwritten by them.
  const settings = invoke("get_settings").catch(() => ({}));
  invoke("settings_info").then(knowFsInfo, () => {});
  let shown = $state(SHOWN);
  let shelfPrefs = $state(SHELF.default);
  const changed = { shown: false, shelf: false };
  settings.then((s) => {
    if (!changed.shown && s?.fs_details) shown = s.fs_details;
    if (!changed.shelf && s?.fs_bookshelf) shelfPrefs = s.fs_bookshelf;
  });
  // Shelves: the rows, the focused one (the first holding the game when
  // the remembered one doesn't), and the game's place in it. With no game
  // picked yet, the focus starts on the first shelf's first box.
  let shelfRow = $state(0);
  let pName = $derived(platforms[pIndex]?.name ?? "All games");
  let layout = $derived(theme.applied?.layout ?? "grid");
  let shelves = $derived(
    layout === "shelves" ? shelvesFor(games, marks, pName) : layout === "bookshelf" ? bookcase(games, marks, shelfPrefs, pName, gameKey) : [],
  );
  let game = $derived(games.find((r) => r.id === gameId) ?? shelves[0]?.games[0] ?? games[0] ?? null);
  let gIndex = $derived(Math.max(0, games.indexOf(game)));
  const holds = (s) => s?.games.some((r) => r.id === game?.id);
  let sRow = $derived(holds(shelves[shelfRow]) ? shelfRow : Math.max(0, shelves.findIndex(holds)));
  let sCol = $derived(Math.max(0, shelves[sRow]?.games.findIndex((r) => r.id === game?.id) ?? 0));
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

  // The theme (theme.svelte.js' followTheme), the system's light or dark.
  let root = $state(null);
  const system = followTheme(() => root);
  let dark = $derived(system.dark);
  // The wheel and hero rows are led by the game's fanart: theirs is always on.
  const ART_LED = ["wheel", "hero", "hero-bottom"];
  let backdrop = $derived(screen === "home" && (ART_LED.includes(layout) || shown.includes("backdrop")) ? artSrc(game?.pictures?.hero) : null);
  // The bookshelf: the box taken off it (Accept), turned a quarter at a
  // time; Accept again opens the game's page, B puts it back.
  let boxOpen = $state(false);
  let boxTurn = $state(0);
  // Put back when the layout or the screen changes under it.
  $effect(() => {
    if (layout !== "bookshelf" || screen !== "home") boxOpen = false;
  });
  // The platforms' colours (as the splashes'), for boxes without art.
  let accents = $state({});
  invoke("all_systems").then((all) => (accents = Object.fromEntries(all.map((s) => [s.key, s.accent]))), () => {});
  const act = {
    themes: () => theme.view,
    details: () => shown,
    bookshelf: () => (layout === "bookshelf" ? shelfPrefs : null),
    setBookshelf: (b) => {
      changed.shelf = true;
      shelfPrefs = b;
      invoke("set_fs_bookshelf", { shelf: b }).then((kept) => (shelfPrefs = kept), () => {});
      return b;
    },
    // Shown at once, then kept (as the core cleaned it).
    setDetails: (s) => {
      changed.shown = true;
      shown = s;
      invoke("set_fs_details", { shown: s }).then((kept) => (shown = kept), () => {});
      return s;
    },
    theme: (s) => void pickTheme(s),
    // Get themes: fetched, then shown (the menu says so meanwhile).
    store: () => later("Get themes", "Fetching the list…", () => invoke("themes_available").then((a) => storeMenu(act, a), () => storeMenu(act, null))),
    install: (t) => {
      run(`Getting the ${t.name} theme…`, "theme_install", { id: t.id }).then(async (done) => {
        if (done === null) return;
        await loadThemes();
        // Only if nothing else is open by now (it may have taken minutes).
        if (!menu && !playing) menu = ask(`${t.name} is in. Use it now?`, () => act.useTheme(t.id));
      });
    },
    useTheme: (id) => void pickTheme({ ...theme.view.selection, theme: id, scheme: "", layout: "" }),
    leave,
    quit: () => void invoke("quit"),
    sort: (s) => void (sort = s),
    art: (a) => void (art = a),
    desktop: (g) => void desktop(g.id),
    play: (g) => void start(g),
    favourite: (g) => void toggleFavourite(g),
    search: () => void (screen = "search"),
    music: () => (music.tracks.length ? music : null),
    nextTrack: () => nextTrack(),
    toggleMusic: () => toggleMusic(),
    volume: (v) => setVolume(v),
    // The emulator for this game: fetched, then a menu of the installed ones.
    emulator: (g) =>
      later("Emulator", "Looking…", () => invoke("game_emulator", { system: g.system, title: g.title }).then((e) => emulatorMenu(act, g, e))),
    setEmulator: (g, name) => void invoke("set_game_emulator", { system: g.system, title: g.title, name }).catch(() => {}),
  };
  // A menu that has to fetch first: a placeholder now, the menu once
  // `load` answers, but only if the placeholder is still the one open (B,
  // or another menu, meanwhile wins). Pressing the placeholder keeps it.
  let waits = 0;
  function later(title, waiting, load) {
    const id = ++waits;
    const holder = { title, index: 0, waiting: id, items: [{ label: waiting, run: () => menu }] };
    load()
      .then((m) => menu?.waiting === id && (menu = m))
      .catch(() => menu?.waiting === id && (menu = null));
    return holder;
  }

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
          { id: "favourite", label: isFavourite(game) ? "★ Favourite" : "☆ Favourite", run: () => toggleFavourite(game) },
          ...(game.playable ? [{ id: "emulator", label: "Emulator…", run: () => (menu = act.emulator(game)) }] : []),
          { id: "back", label: "Back", run: () => (screen = "home") },
        ]
      : [],
  );

  function pressMenu(i) {
    const next = menu?.items[i]?.run();
    menu = next ?? null;
  }

  // Attract mode and the screensaver (Settings; off unless set there).
  $effect(() => {
    Promise.all([invoke("start_options_now").catch(() => ({})), settings]).then(([o, s]) => {
      const attract = o?.attract_seconds ? o.attract_seconds * 1000 : (s?.attract_minutes ?? 0) * 60000;
      setIdle(attract, (s?.screensaver_minutes ?? 0) * 60000);
    });
    return () => pauseIdle(true);
  });
  $effect(() => pauseIdle(!!playing));
  // Music while the mode is open, quiet while a game runs.
  $effect(() => {
    startMusic();
    return () => stopMusic();
  });
  $effect(() => pauseMusic(!!playing));
  // Woken on a game: it is the one focused.
  function wokeOn(g) {
    if (!g) return;
    platformKey = "*";
    gameId = g.id;
    screen = "home";
    focus = "grid";
  }
  let attractGame = null;

  let finder = $state(null);
  // A search result: its platform, and its page.
  function openFound(g) {
    platformKey = "*";
    gameId = g.id;
    screen = "game";
    button = 0;
  }

  // The box taken out: on to the game's page.
  function openBox() {
    boxOpen = false;
    screen = "game";
    button = 0;
  }

  function pickShelf(row, col) {
    shelfRow = row;
    gameId = shelves[row]?.games[col]?.id ?? gameId;
  }

  // One action from the pad or the keyboard.
  function handle(action) {
    if (playing) return;
    if (poke()) return wokeOn(attractGame);
    if (screen === "search" && !menu) return finder?.pad(action);
    if (menu) {
      if (action === "back") menu = null;
      else if (action === "accept") pressMenu(menu.index);
      else menu.index = listMove(menu.index, menu.items.length, action);
      return;
    }
    if (action === "menu") {
      // Themes added to the folder since show up in Start → Theme.
      loadThemes();
      return void (menu = mainMenu(act, { sort, art }));
    }
    if (screen === "game") {
      if (action === "back") screen = "home";
      else if (action === "accept") buttons[button]?.run();
      else if (action === "north") menu = optionsMenu(act, game, versionsOf(game));
      else if (action === "context") menu = contextMenu(act, game, versionsOf(game));
      else if (action === "west") toggleFavourite(game);
      else if (action === "left" || action === "right") {
        // Along the shelf it is on (the shelf layouts), else the games.
        const order = shelves[sRow]?.games ?? games;
        const i = order.indexOf(game) + (action === "left" ? -1 : 1);
        if (i >= 0 && i < order.length) gameId = order[i].id;
        button = 0;
      } else button = listMove(button, buttons.length, action === "up" ? "left" : action === "down" ? "right" : action);
      return;
    }
    if (action === "west" && focus === "grid") return void toggleFavourite(game);
    if (boxOpen) {
      if (action === "left" || action === "right") boxTurn += action === "left" ? -1 : 1;
      else if (action === "back") boxOpen = false;
      else if (action === "accept") openBox();
      else if (action === "north") menu = optionsMenu(act, game, versionsOf(game));
      return;
    }
    const shelf = { row: sRow, col: sCol, lengths: shelves.map((s) => s.games.length), wrap: shelfPrefs.direction === "vertical" };
    const r = homeAction({ focus, platform: pIndex, platforms: platforms.length, game: gIndex, count: games.length, cols, rows, layout, shelf }, action);
    if (r.platform !== undefined) {
      platformKey = platforms[r.platform].system;
      gameId = null;
      shelfRow = 0;
    }
    if (r.game !== undefined) gameId = games[r.game]?.id ?? null;
    if (r.shelf) pickShelf(r.shelf.row, r.shelf.col);
    if (r.focus) focus = r.focus;
    if (r.intent === "open" && layout === "bookshelf") ((boxOpen = true), (boxTurn = 0));
    else if (r.intent === "open") {
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

  // Quit's chord, from romforge's hotkeys table.
  let quitChord = $state("");
  invoke("hotkeys_view", { emulators: [] })
    .then((v) => (quitChord = v.quit_chord))
    .catch(() => {});

  let help = $derived(helpBar({ menu, screen, buttons, button, quitChord, boxOpen, focus, layout, fav: isFavourite(game) }));
</script>

<svelte:window
  bind:innerWidth={winW}
  bind:innerHeight={winH}
  onmousemove={() => poke() && wokeOn(attractGame)}
  onkeydown={(e) => {
    if (idle.mode) return void (e.preventDefault(), poke(), wokeOn(attractGame));
    if (screen === "search" && !menu) return finder?.key(e);
    if (e.key === "/" && !menu) return void (e.preventDefault(), (screen = "search"));
    const a = keyAction(e.key);
    if (!a) return;
    e.preventDefault();
    handle(a);
  }}
/>

<div class="fs" data-fs-root data-fs-screen={screen} data-fs-layout={layout} data-fs-shelf-details={layout === "bookshelf" && shelfPrefs.details !== "top" ? shelfPrefs.details : undefined} data-fs-aspect={aspect} bind:this={root}>
  {#if backdrop}<img class="backdrop" src={backdrop} alt="" data-fs-detail="backdrop" />{/if}
  {#if screen === "home"}
    <header class="top" data-fs-wordmark><Wordmark height="3.2vh" /></header>
    <FsCarousel {platforms} index={pIndex} focused={focus === "carousel" && !menu} {icons} {iconStyle} pick={(i) => ((platformKey = platforms[i].system), (gameId = null), (focus = "carousel"))} />
    <main class="home">
      {#if layout === "list"}
        <FsList {games} index={gIndex} focused={focus === "grid" && !menu} {art} {icons} fav={marks.favourites} bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      {:else if layout === "wheel"}
        <FsWheel {games} index={gIndex} focused={focus === "grid" && !menu} fav={marks.favourites} bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      {:else if layout === "hero" || layout === "hero-bottom"}
        <FsTiles {games} index={gIndex} focused={focus === "grid" && !menu} {art} {icons} fav={marks.favourites} bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      {:else if layout === "bookshelf"}
        <FsBookshelf {shelves} {accents} row={sRow} col={sCol} focused={focus === "grid" && !menu && !boxOpen} fav={marks.favourites} bind:rows pick={(r, c) => (pickShelf(r, c), (focus = "grid"))} open={() => handle("accept")} />
      {:else if layout === "shelves"}
        <FsShelves {dark} {shelves} row={sRow} col={sCol} focused={focus === "grid" && !menu} {art} {icons} fav={marks.favourites} bind:rows pick={(r, c) => (pickShelf(r, c), (focus = "grid"))} open={() => handle("accept")} />
      {:else if layout === "cover-flow"}
        <FsFlow {dark} {games} index={gIndex} focused={focus === "grid" && !menu} {art} {icons} fav={marks.favourites} bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      {:else}
        <FsGrid {dark} {games} index={gIndex} focused={focus === "grid" && !menu} {art} {icons} fav={marks.favourites} bind:cols bind:rows pick={(i) => ((gameId = games[i].id), (focus = "grid"))} open={() => handle("accept")} />
      {/if}
      <FsInfo {game} {detail} {shown} />
    </main>
  {:else if screen === "search"}
    <header class="top" data-fs-wordmark><Wordmark height="3.2vh" /></header>
    <FsSearch bind:this={finder} {all} open={openFound} close={() => (screen = "home")} />
  {:else if game}
    <FsGame {game} {detail} {buttons} focus={button} press={(i) => buttons[i]?.run()} />
  {/if}
  <FsHelp items={help} pad={fs.padSeen} />
  {#if boxOpen && layout === "bookshelf" && screen === "home" && game}
    <FsBox {game} {detail} turn={boxTurn} accent={accents[game.system]} turnBy={(d) => (boxTurn += d)} close={() => (boxOpen = false)} open={openBox} />
  {/if}
  {#if menu}<FsMenu {menu} press={pressMenu} />{/if}
  {#if idle.mode}<FsAttract {all} saver={idle.mode === "saver"} shown={(g) => (attractGame = g)} />{/if}
</div>
