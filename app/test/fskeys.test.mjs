import { test } from "node:test";
import assert from "node:assert/strict";
import { keyAt, keyMove, ROWS, search, typed } from "../src/fullscreen/fskeys.js";

test("the d-pad moves through the keys", () => {
  assert.deepEqual(keyMove({ row: 0, col: 0 }, "left"), { row: 0, col: 9 }, "wraps in the row");
  assert.deepEqual(keyMove({ row: 0, col: 9 }, "right"), { row: 0, col: 0 });
  assert.deepEqual(keyMove({ row: 0, col: 3 }, "up"), { row: 4, col: 1 }, "up from the top: the last row, same place");
  assert.equal(keyAt(keyMove({ row: 3, col: 9 }, "down")), "done");
  assert.deepEqual(keyMove({ row: 4, col: 0 }, "up"), { row: 3, col: 1 });
  for (let r = 0; r < ROWS.length; r++) for (let c = 0; c < ROWS[r].length; c++) for (const a of ["up", "down", "left", "right"]) assert.ok(keyAt(keyMove({ row: r, col: c }, a)));
});

test("typing and finding games", () => {
  let q = "";
  for (const k of ["D", "I", "S", "C", "space", "W"]) q = typed(q, k);
  assert.equal(q, "disc w");
  assert.equal(typed(q, "delete"), "disc ");
  assert.equal(typed(q, "clear"), "");
  const all = [{ title: "Discworld (CD)", name: { title: "Discworld" } }, { title: "Darkseed" }, { title: "Mega Disc Wars" }];
  assert.deepEqual(search(all, "disc").map((r) => r.title), ["Discworld (CD)", "Mega Disc Wars"], "starts-with first");
  assert.deepEqual(search(all, "wo disc").map((r) => r.title), ["Discworld (CD)"], "every word");
  assert.deepEqual(search(all, "  "), []);
});
