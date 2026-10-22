// The shared labels and sizes (node --test app/test).
import { test } from "node:test";
import assert from "node:assert/strict";
import { displayTitle, duration, played } from "../src/labels.js";
import { size } from "../src/lib.js";

test("a game is shown by its clean title, else its label, else its DAT title", () => {
  assert.equal(displayTitle({ name: { title: "Zelda" }, label: "Set", title: "Zelda (USA)" }), "Zelda");
  assert.equal(displayTitle({ label: "Set", title: "Zelda (USA)" }), "Set");
  assert.equal(displayTitle({ title: "Zelda (USA)" }), "Zelda (USA)");
  assert.equal(displayTitle(null), "");
});

test("durations in seconds, minutes or hours", () => {
  assert.equal(duration(45), "45 s");
  assert.equal(duration(600), "10 min");
  assert.equal(duration(7200), "2.0 h");
  assert.equal(duration(20, { seconds: false }), "1 min");
  assert.match(played({ seconds: 20, sessions: 1, last_played: 0 }), /^1 min · once · last /);
});

test("sizes in binary units", () => {
  assert.equal(size(512), "512 B");
  assert.equal(size(1536), "1.5 KiB");
  assert.equal(size(3 * 2 ** 20), "3.0 MiB");
  assert.equal(size(5 * 2 ** 30), "5.0 GiB");
});
