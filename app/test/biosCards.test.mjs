// The BIOS page's platform cards (node --test app/test).
import { test } from "node:test";
import assert from "node:assert/strict";
import { matchedBy, planPlaces, summary } from "../src/biosCards.js";

const file = (name, place, state) => ({ name, place, state });
const ps1 = {
  emulators: [
    { ready: true, profiled: true, files: [file("scph5501.bin", "/B/RetroArch/scph5501.bin", "accepted"), file("scph5500.bin", "/B/RetroArch/scph5500.bin", "missing")] },
    { ready: false, profiled: true, files: [file("scph5501.bin", "/B/duckstation/scph5501.bin", "missing"), file("SCPH5500.BIN", "/B/RetroArch/scph5500.bin", "missing")] },
  ],
};
const plan = {
  files: [
    { shown: "drop/a.bin", matched: "accepted", targets: [{ place: "/B/duckstation/scph5501.bin", state: "new" }, { place: "/B/RetroArch/scph5501.bin", state: "already_there" }] },
    { shown: "drop/scph5500.bin", matched: "name_only", targets: [{ place: "/B/RetroArch/scph5500.bin", state: "new" }] },
    { shown: "drop/notes.txt", matched: "unknown", targets: [] },
  ],
};

test("a card counts files by name, places, what is there and what the import fills by hash", () => {
  assert.deepEqual(summary(ps1, planPlaces(plan)), { files: 2, places: 3, have: 1, incoming: 1, ready: 1, emulators: 2 });
  assert.equal(summary(ps1).incoming, 0);
});

test("a place says how its file matched, there or in the import", () => {
  const planned = planPlaces(plan);
  assert.equal(matchedBy(ps1.emulators[0].files[0], planned.get("/B/RetroArch/scph5501.bin")), "Hash");
  assert.equal(matchedBy(ps1.emulators[1].files[0], planned.get("/B/duckstation/scph5501.bin")), "Hash (in this import)");
  assert.equal(matchedBy(ps1.emulators[0].files[1], planned.get("/B/RetroArch/scph5500.bin")), "Name (in this import, if ticked)");
  assert.equal(matchedBy(file("x", "/B/x", "missing"), undefined), "—");
  assert.equal(planned.get("/B/duckstation/scph5501.bin").from, "drop/a.bin");
});
