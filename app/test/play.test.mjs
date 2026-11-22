import { test } from "node:test";
import assert from "node:assert/strict";
import { playChoice } from "../src/play.js";
import { versionKind, versionName } from "../src/versions.js";

const rows = [
  { id: "f:1", title: "Mario (USA)", main: true },
  { id: "f:2", title: "Mario (Japan)", category: "region" },
];
const calls = (answer) => async (cmd, args) => (assert.equal(cmd, "play_choice"), answer(args));

test("Play starts the row the core names, else offers the versions", async () => {
  let offered = null;
  const pick = (vs) => (offered = vs);
  assert.equal(await playChoice(rows[0], calls(() => ({ run: "f:2", versions: [] })), rows, pick), rows[1], "Always run's version");
  assert.equal(offered, null);
  assert.equal(await playChoice(rows[0], calls(() => ({ run: null, versions: rows })), rows, pick), null);
  assert.deepEqual(offered, rows, "asked which");
  assert.equal(await playChoice(rows[0], calls(() => null), rows, pick), null, "a failed call starts nothing");
  assert.equal(await playChoice(null, calls(() => assert.fail()), rows, pick), null);
});

test("the picker names each version by its whole title and kind", () => {
  assert.equal(versionName({ title: "sf2", label: "Street Fighter II (World 910522)" }), "Street Fighter II (World 910522)");
  assert.equal(versionName(rows[1]), "Mario (Japan)");
  assert.equal(versionKind(rows[0]), "Main version");
  assert.equal(versionKind(rows[1]), "Other region");
  assert.equal(versionKind({ category: "patch_hack" }), "Hack");
});
