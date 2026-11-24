// The BIOS page's platform cards and their dialog: what each system's
// emulators load (the BIOS report) against the import plan on screen.

// A file in the system's places is one you have in these states.
const HAVE = new Set(["accepted", "present", "placed_by_you", "incomplete"]);

// The import plan's files by the place each would fill: where the file is
// now (`shown`) and how it matched. A place two dropped files fit keeps the
// one that goes there (`new`), else the first.
export function planPlaces(plan) {
  const out = new Map();
  for (const f of plan?.files ?? []) {
    if (f.matched === "unknown") continue;
    for (const t of f.targets) {
      const had = out.get(t.place);
      if (had && (had.state === "new" || t.state !== "new")) continue;
      out.set(t.place, { from: f.shown, matched: f.matched, state: t.state });
    }
  }
  return out;
}

// One card's numbers: the BIOS files (by name) and the places they go in,
// how many places hold a file now, and how many the plan would fill with a
// file matched by its hash.
export function summary(system, planned = new Map()) {
  const names = new Set();
  const places = new Map();
  for (const e of system.emulators)
    for (const f of e.files) {
      names.add(f.name.toLowerCase());
      if (!places.has(f.place) || HAVE.has(f.state)) places.set(f.place, f.state);
    }
  let have = 0;
  let incoming = 0;
  for (const [place, state] of places) {
    if (HAVE.has(state)) have++;
    else if (planned.get(place)?.matched === "accepted" && planned.get(place)?.state === "new") incoming++;
  }
  const ready = system.emulators.filter((e) => e.ready && e.profiled).length;
  return { files: names.size, places: places.size, have, incoming, ready, emulators: system.emulators.length };
}

// How a file at its place matched, in a few words; `planned` is the plan's
// file for that place, if any.
export function matchedBy(f, planned) {
  switch (f.state) {
    case "accepted":
      return "Hash";
    case "present":
      return "Name (the list has no hash)";
    case "placed_by_you":
      return "Name, placed by you";
    case "wrong_hash":
      return "Name only: its hash is wrong";
    case "incomplete":
      return "Hash, some members missing";
  }
  if (!planned) return "—";
  return planned.matched === "accepted" ? "Hash (in this import)" : "Name (in this import, if ticked)";
}
