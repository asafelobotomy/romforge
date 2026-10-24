import { test } from "node:test";
import assert from "node:assert/strict";
import { helpBar } from "../src/fullscreen/fshelp.js";

test("the help bar names what each screen's buttons do", () => {
  const home = helpBar({ screen: "home", focus: "grid", layout: "bookshelf", fav: true });
  assert.deepEqual(home[0], ["accept", "Take out"]);
  assert.deepEqual(home[1], ["west", "Unfavourite"]);
  assert.deepEqual(helpBar({ screen: "home", boxOpen: true })[0], ["side", "Turn"]);
  assert.deepEqual(helpBar({ menu: {}, screen: "game" })[2], ["back", "Close"], "a menu first");
  const game = helpBar({ screen: "game", buttons: [{ label: "Play" }], quitChord: "Select+Start" });
  assert.deepEqual(game[0], ["accept", "Play"]);
  assert.deepEqual(game.at(-1), ["Select+Start", "Quits a game"]);
});
