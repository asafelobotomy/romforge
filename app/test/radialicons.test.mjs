// The radial ring's icons (node --test app/test).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ICONS, iconFor } from "../src/radial/icons.js";

// The ring's actions, as the core lists them (RADIAL in hotkeys.rs).
const core = readFileSync(new URL("../../crates/romforge-core/src/hotkeys.rs", import.meta.url), "utf8");
const ring = core
  .match(/pub const RADIAL: &\[Action\] = &\[([^\]]*)\]/)[1]
  .match(/Action::(\w+)/g)
  .map((a) => a.slice(8).replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());

test("every action in the ring has an icon", () => {
  assert.ok(ring.length >= 4, ring.join());
  for (const a of ring) assert.ok(iconFor(a), `no icon for ${a}`);
});

test("icons are plain stroke paths, and an unknown action has none", () => {
  for (const [a, d] of Object.entries(ICONS)) assert.match(d, /^[MmLlHhVvAaZz0-9 .\-,]+$/, a);
  assert.equal(iconFor("nope"), null);
});
