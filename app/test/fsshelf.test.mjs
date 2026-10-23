import { test } from "node:test";
import assert from "node:assert/strict";
import { bookcase, bookshelfMenu, shelfSorted, sizeOf, shelfScale, shelfFirst, shelfFits, SHELF_MAX, SHELF_MAX_BESIDE } from "../src/fullscreen/fsshelf.js";

test("boxes keep their real sizes, the tallest filling the shelf", () => {
  const games = [{ box_size: [135, 190, 14] }, { box_size: [142, 125, 10] }, {}];
  assert.equal(shelfScale(games, 380), 2, "190 mm in 380 px");
  assert.deepEqual(sizeOf({}), [130, 185, 35], "a row with no size stands as a cartridge box");
});

test("the shelf moves only when the focus would leave it", () => {
  const widths = [10, 10, 10, 10, 10, 10, 10, 10];
  assert.equal(shelfFirst(widths, 2, 0, 45), 0, "in view, with one beyond");
  assert.equal(shelfFirst(widths, 4, 0, 45), 2, "just enough to keep one beyond");
  assert.equal(shelfFirst(widths, 7, 0, 45), 4, "the last needs none beyond");
  assert.equal(shelfFirst(widths, 1, 4, 45), 1, "back to the focus");
  assert.equal(shelfFits(widths, 2, 45), 4);
  assert.equal(shelfFits([100], 0, 45), 1, "at least one");
});

const key = (r) => `${r.system}\t${r.title}`;
const many = (system, n) => Array.from({ length: n }, (_, i) => ({ system, system_name: system, title: `${system} ${i}` }));
const none = { favourites: new Set(), played: new Map() };

test("the bookcase: shelves by platform or mixed, cut at the most a shelf holds", () => {
  const games = [...many("SNES", 3), ...many("Genesis", SHELF_MAX + 5)];
  const flat = bookcase(games, none, {}, "All games", key);
  assert.deepEqual(flat.map((s) => s.name), ["Genesis", "SNES"], "endless, a shelf a platform, by name");
  assert.equal(flat[0].games.length, SHELF_MAX + 5);
  const tall = bookcase(games, none, { direction: "vertical" }, "All games", key);
  assert.deepEqual(tall.map((s) => s.name), ["Genesis 1/2", "Genesis 2/2", "SNES"]);
  assert.equal(tall[0].games.length, SHELF_MAX);
  const mixed = bookcase(games, none, { direction: "vertical", group: "mixed" }, "All games", key);
  assert.deepEqual(mixed.map((s) => s.games.length), [SHELF_MAX, 8]);
});

test("the bookcase's favourites and lately played shelves, when asked for", () => {
  const games = many("SNES", 4);
  const marks = { favourites: new Set([key(games[2])]), played: new Map([[key(games[0]), { last_played: 5 }], [key(games[3]), { last_played: 9 }]]) };
  const s = bookcase(games, marks, { favourites: true, recent: true }, "SNES", key);
  assert.deepEqual(s.map((x) => x.key), ["fav", "recent", "p:SNES"]);
  assert.deepEqual(s[1].games.map((r) => r.title), ["SNES 3", "SNES 0"], "latest first");
  assert.deepEqual(bookcase(games, none, { favourites: true, recent: true }, "SNES", key).map((x) => x.key), ["p:SNES"], "empty ones left out");
});

test("Start → Bookshelf changes a choice at once and stays on it", () => {
  let kept = null;
  const act = { setBookshelf: (b) => (kept = b) };
  const m = bookshelfMenu(act, { direction: "horizontal", group: "platform", favourites: false, recent: true });
  assert.equal(m.items[0].label, "Shelves: One endless shelf each");
  const next = m.items[0].run();
  assert.equal(kept.direction, "vertical");
  assert.equal(next.index, 0);
  assert.equal(m.items[3].run().items[3].label, "Favourites shelf: On");
  assert.equal(kept.recent, true, "the rest kept");
});

test("the shelves' order: name, release date either way, most played", () => {
  const g = [{ system: "A", title: "Cobra", year: 1990 }, { system: "A", title: "alpha", year: 1995 }, { system: "A", title: "Bravo" }];
  const m = { favourites: new Set(), played: new Map([[key(g[2]), { seconds: 90 }], [key(g[0]), { seconds: 10 }]]) };
  const titles = (sort) => shelfSorted(g, sort, m, key).map((r) => r.title);
  assert.deepEqual(titles("title"), ["alpha", "Bravo", "Cobra"]);
  assert.deepEqual(titles("released"), ["Cobra", "alpha", "Bravo"], "no year last");
  assert.deepEqual(titles("newest"), ["alpha", "Cobra", "Bravo"], "no year last");
  assert.deepEqual(titles("played"), ["Bravo", "Cobra", "alpha"]);
});

test("with the details beside them, bookcase shelves hold fewer", () => {
  const games = many("SNES", 20);
  const beside = bookcase(games, none, { direction: "vertical", details: "left" }, "SNES", key);
  assert.deepEqual(beside.map((s) => s.games.length), [SHELF_MAX_BESIDE, 5]);
  assert.equal(bookcase(games, none, { direction: "vertical" }, "SNES", key).length, 1, "above them, 30 a shelf");
  assert.equal(bookcase(games, none, { details: "right" }, "SNES", key)[0].games.length, 20, "an endless shelf stays endless");
  const m = bookshelfMenu({ setBookshelf: (b) => b }, { details: "top" });
  assert.equal(m.items[5].run().items[5].label, "Details: On the left");
});
