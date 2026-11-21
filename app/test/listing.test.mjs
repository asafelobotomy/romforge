import { test } from "node:test";
import assert from "node:assert/strict";
import { grouped, libraryRows, onPlatform } from "../src/listing.js";
import { targets } from "../src/selection.js";
import { sorted } from "../src/sort.js";

const row = (id, title, group, main = true) => ({ id, title, group, main, system: "snes", status: "verified" });
const lib = [
  row("b", "Beta", "g:beta"),
  row("b2", "Beta (Europe)", "g:beta", false),
  row("a", "Alpha", null),
  row("c", "Gamma", "g:gamma"),
  row("c2", "Gamma (Japan)", "g:gamma", false),
];

test("the library is sorted once, and the filter and folds cut from it", () => {
  const g = grouped(lib, { key: "title", dir: 1 });
  assert.deepEqual(g.all.map((r) => r.id), ["a", "b", "b2", "c", "c2"]);
  assert.deepEqual(g.members.get("g:beta").map((r) => r.id), ["b", "b2"]);
  const closed = () => false;
  assert.deepEqual(libraryRows(g, "*", "", closed).map((r) => r.id), ["a", "b", "c"]);
  assert.deepEqual(libraryRows(g, "*", "JAPAN", closed).map((r) => r.id), ["c"], "a group listed when any version matches");
});

test("a folded game brings its versions to the jobs", () => {
  const g = grouped(lib, { key: "title", dir: 1 });
  const rows = libraryRows(g, "*", "", () => false);
  assert.deepEqual(targets(new Set(["b", "a"]), rows, g.members, () => false).sort(), ["a", "b", "b2"]);
  assert.deepEqual(targets(new Set(["x"]), rows, g.members, () => false), [], "not listed, not a target");
});

test("each value is read once when sorting", () => {
  let reads = 0;
  const items = Array.from({ length: 200 }, (_, i) => ({ n: (i * 37) % 200 }));
  const out = sorted(items, { key: "n", dir: -1 }, { n: (x) => (reads++, x.n) });
  assert.equal(reads, 200);
  assert.equal(out[0].n, 199);
  assert.equal(out.at(-1).n, 0);
});

test("a game is on a platform's list, folded versions too", () => {
  const g = grouped([...lib, { ...row("u", "Unknown", null), unmatched: true }], { key: "title", dir: 1 });
  assert.ok(onPlatform(g, "*", "b2"), "a folded version is still there");
  assert.ok(onPlatform(g, "snes", "a"));
  assert.ok(!onPlatform(g, "nes", "a"), "another platform's game");
  assert.ok(!onPlatform(g, "*", "u") && onPlatform(g, "", "u"), "unmatched files only under Unidentified");
});
