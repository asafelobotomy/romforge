// The bookshelf layout's arithmetic: boxes stood side by side on a shelf,
// their spines out, each at its real size (a row's box_size: width,
// height, depth in millimetres). Pure, so it is tested in Node.

import { displayTitle } from "../labels.js";

// A box's size, with a cartridge box's for a row that has none.
export const sizeOf = (r) => (r?.box_size?.[1] ? r.box_size : [130, 185, 35]);

// Pixels per millimetre so the tallest of `games` fills `height`.
export function shelfScale(games, height) {
  const tallest = Math.max(1, ...games.map((r) => sizeOf(r)[1]));
  return height / tallest;
}

// The first box drawn, so that `index` is in view on a shelf `width`
// wide: `first` stays unless the focus would leave the view, then moves
// just enough (keeping one box beyond the focus where there is one).
// `widths` are the boxes' spines with the gap after each, in pixels.
export function shelfFirst(widths, index, first, width) {
  if (index < first) return index;
  const want = Math.min(index + 1, widths.length - 1);
  let f = first;
  let span = 0;
  for (let i = f; i <= want; i++) span += widths[i];
  while (f < index && span > width) span -= widths[f++];
  return f;
}

// How many boxes from `first` fit on a shelf `width` wide (at least one).
export function shelfFits(widths, first, width) {
  let n = 0;
  let span = 0;
  while (first + n < widths.length && span + widths[first + n] <= width) span += widths[first + n++];
  return Math.max(1, n);
}

// The bookshelf's choices, named, its default and the most boxes a
// bookcase shelf holds with the details above the shelves and beside them:
// the core's (settings::bookshelf_info, sent by settings_info: knowShelf).
// Until then a default that stands the shelves, and no lists.
export let SHELF = {
  default: { direction: "horizontal", group: "platform", favourites: false, recent: false, sort: "title", details: "top" },
  max: 30,
  max_beside: 15,
  directions: [],
  groups: [],
  sorts: [],
  details: [],
};
export function knowShelf(info) {
  if (info?.default) SHELF = info;
}
export const shelfMax = (p) => ((p?.details ?? "top") === "top" ? SHELF.max : SHELF.max_beside);

// `games` in the bookshelf's order `sort`: by title, by release year
// (oldest or newest first, games without one last), or longest played
// first; ties by title.
export function shelfSorted(games, sort, marks, keyOf) {
  const title = (r) => displayTitle(r).toLowerCase();
  const seconds = (r) => marks.played.get(keyOf(r))?.seconds ?? 0;
  const by = {
    released: (a, b) => (a.year ?? 9999) - (b.year ?? 9999),
    newest: (a, b) => (b.year ?? -1) - (a.year ?? -1),
    played: (a, b) => seconds(b) - seconds(a),
  }[sort];
  return [...games].sort((a, b) => (by ? by(a, b) : 0) || title(a).localeCompare(title(b)));
}

// The bookshelf's shelves for `all` (the platform picked's games, or
// every platform's for All games): its favourites first and those played
// lately next (latest first) when asked for, then the games by platform
// (each platform's own, by name) or all mixed under `name`, in the order
// asked for. In the bookcase
// (`vertical`) each run is cut into shelves of shelfMax, numbered; else
// each is one endless shelf. Empty ones aren't there. `keyOf(r)` keys a
// game as favourites and play time are kept.
export function bookcase(all, marks, prefs, name, keyOf) {
  const p = { ...SHELF.default, ...prefs };
  const games = shelfSorted(all, p.sort, marks, keyOf);
  const max = shelfMax(p);
  const played = (r) => marks.played.get(keyOf(r));
  const runs = [];
  if (p.favourites) runs.push({ key: "fav", name: "Favourites", games: games.filter((r) => marks.favourites.has(keyOf(r))) });
  if (p.recent) {
    const recent = games.filter(played).sort((a, b) => played(b).last_played - played(a).last_played);
    runs.push({ key: "recent", name: "Recently played", games: recent.slice(0, max) });
  }
  if (p.group === "platform") {
    const by = new Map();
    for (const r of games) {
      if (!by.has(r.system)) by.set(r.system, { key: `p:${r.system}`, name: r.system_name || r.system, games: [] });
      by.get(r.system).games.push(r);
    }
    runs.push(...[...by.values()].sort((a, b) => a.name.localeCompare(b.name)));
  } else runs.push({ key: "all", name, games });
  const out = [];
  for (const run of runs.filter((x) => x.games.length)) {
    if (p.direction !== "vertical") {
      out.push(run);
      continue;
    }
    const n = Math.ceil(run.games.length / max);
    for (let i = 0; i < n; i++) {
      out.push({ key: `${run.key}:${i}`, name: n > 1 ? `${run.name} ${i + 1}/${n}` : run.name, games: run.games.slice(i * max, (i + 1) * max) });
    }
  }
  return out;
}

// Start → Bookshelf: each choice and its value; pressing one changes it
// at once (and it is kept); `index` stays on the item pressed.
export function bookshelfMenu(act, b, index = 0) {
  const p = { ...SHELF.default, ...b };
  const other = (list, v) => (list.length ? list[(list.findIndex((x) => x.value === v) + 1) % list.length].value : v);
  const label = (list, v) => list.find((x) => x.value === v)?.label ?? v;
  const again = (change, i) => bookshelfMenu(act, act.setBookshelf({ ...p, ...change }), i);
  return {
    title: "Bookshelf",
    index,
    items: [
      { label: `Shelves: ${label(SHELF.directions, p.direction)}`, run: () => again({ direction: other(SHELF.directions, p.direction) }, 0) },
      { label: `All games: ${label(SHELF.groups, p.group)}`, run: () => again({ group: other(SHELF.groups, p.group) }, 1) },
      { label: `Order: ${label(SHELF.sorts, p.sort)}`, run: () => again({ sort: other(SHELF.sorts, p.sort) }, 2) },
      { label: `Favourites shelf: ${p.favourites ? "On" : "Off"}`, run: () => again({ favourites: !p.favourites }, 3) },
      { label: `Recently played shelf: ${p.recent ? "On" : "Off"}`, run: () => again({ recent: !p.recent }, 4) },
      { label: `Details: ${label(SHELF.details, p.details)}`, run: () => again({ details: other(SHELF.details, p.details) }, 5) },
    ],
  };
}
