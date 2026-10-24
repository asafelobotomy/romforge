import { test } from "node:test";
import assert from "node:assert/strict";
import { DETAILS, SHOWN, knowDetails, toggled, detailsMenu } from "../src/fullscreen/fsdetails.js";

// As settings_info sends them (the core's settings::fs_details_info).
const NAMES = [["logo", "Logo (else title)"], ["platform", "Platform"], ["released", "Release date"], ["genre", "Genre"], ["overview", "Overview"], ["backdrop", "Fanart backdrop"]];
assert.deepEqual(DETAILS, [], "none until the core has sent them");
knowDetails({ details: NAMES.map(([value, label]) => ({ value, label })), shown: ["logo", "genre", "overview"] });

test("turning a detail on or off keeps the list in order", () => {
  assert.deepEqual(toggled(["overview", "logo"], "genre"), ["logo", "genre", "overview"]);
  assert.deepEqual(toggled(["logo", "genre"], "logo"), ["genre"]);
  assert.deepEqual(toggled([], "backdrop"), ["backdrop"]);
  assert.ok(SHOWN.every((k) => DETAILS.some((d) => d.key === k)));
});

test("Start → Details turns one on or off and stays on it", () => {
  let kept = null;
  const act = { setDetails: (s) => (kept = s) };
  const m = detailsMenu(act, ["logo", "overview"]);
  assert.equal(m.items.length, DETAILS.length);
  assert.equal(m.items[0].label, "Logo (else title): On");
  assert.equal(m.items[1].label, "Platform: Off");
  const next = m.items[1].run();
  assert.deepEqual(kept, ["logo", "platform", "overview"]);
  assert.equal(next.index, 1);
  assert.equal(next.items[1].label, "Platform: On");
});
