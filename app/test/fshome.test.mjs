import { test } from "node:test";
import assert from "node:assert/strict";
import { gamesFor, gameKey, shelvesFor, virtualPlatforms } from "../src/fullscreen/fshome.js";

const row = (system, title, year) => ({ system, title, year });
const all = [row("SNES", "Zelda", 1991), row("SNES", "Mario", 1990), row("NES", "Metroid", 1986)];
const marks = (fav = [], played = []) => ({
  favourites: new Set(fav.map(gameKey)),
  played: new Map(played.map(([r, seconds, last_played]) => [gameKey(r), { seconds, last_played }])),
});

test("history and favourites show only when they have games", () => {
  assert.deepEqual(virtualPlatforms(all, marks()).map((p) => p.system), ["*"]);
  const m = marks([all[2]], [[all[0], 60, 10]]);
  assert.deepEqual(virtualPlatforms(all, m).map((p) => p.system), ["*", "*fav", "*recent", "*most"]);
});

test("each list has its own order", () => {
  const m = marks([all[0]], [[all[0], 600, 10], [all[2], 60, 20]]);
  const titles = (k, sort = "title") => gamesFor(k, all, m, sort).map((r) => r.title);
  assert.deepEqual(titles("*"), ["Mario", "Metroid", "Zelda"]);
  assert.deepEqual(titles("*", "year"), ["Metroid", "Mario", "Zelda"]);
  assert.deepEqual(titles("SNES"), ["Mario", "Zelda"]);
  assert.deepEqual(titles("*fav"), ["Zelda"]);
  assert.deepEqual(titles("*recent"), ["Metroid", "Zelda"], "latest first");
  assert.deepEqual(titles("*most"), ["Zelda", "Metroid"], "longest first");
});

test("shelves: lately played, favourites, then all, empty rows left out", () => {
  assert.deepEqual(shelvesFor(all, marks(), "SNES").map((s) => s.key), ["all"]);
  const m = marks([all[1]], [[all[0], 60, 10], [all[2], 60, 30]]);
  const s = shelvesFor(all, m, "All games");
  assert.deepEqual(s.map((x) => x.name), ["Recently played", "Favourites", "All games"]);
  assert.deepEqual(s[0].games.map((r) => r.title), ["Metroid", "Zelda"], "latest first");
  assert.deepEqual(s[1].games.map((r) => r.title), ["Mario"]);
  assert.equal(s[2].games.length, 3);
});
