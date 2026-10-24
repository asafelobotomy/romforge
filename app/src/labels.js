// What the library and Details call things, and the lines they make of a
// game's facts.

export const statusLabel = {
  verified: "Verified",
  incomplete: "Incomplete",
  candidate: "Candidate",
  wrong_system: "Wrong system or DAT",
  duplicate: "Duplicate",
  by_you: "Identified by you",
  derivative: "Translation or hack (identified by you)",
  detected: "Identified by ScummVM's game list",
  unknown: "Unknown",
  missing: "Not in your library",
  patch: "Patch file",
};
export const artLabel = {
  boxart: "Box art",
  box3d: "3D box",
  title: "Title screen",
  snap: "In-game",
  hero: "Fanart",
  logo: "Logo",
  icon: "Icon",
  spine: "Box spine",
  boxback: "Box back",
  boxfull: "Full cover",
  media: "Cartridge or disc",
  marquee: "Marquee",
  flyer: "Flyer",
  cabinet: "Cabinet",
  video: "Video snap",
};
// Box art, fanart and logo have their own places; the rest sit under Media.
export const mediaKinds = ["box3d", "boxback", "spine", "boxfull", "media", "marquee", "flyer", "cabinet", "title", "snap", "icon"];
export const sourceLabel = {
  libretro: "libretro", screenscraper: "ScreenScraper", steamgriddb: "SteamGridDB", igdb: "IGDB", launchbox: "LaunchBox",
  adb: "Arcade Database", gametdb: "GameTDB",
};
// Where a field's value came from, as Details names it.
export const extdbLabel = {
  openvgdb: "OpenVGDB", launchbox: "LaunchBox", libretro: "libretro", wikidata: "Wikidata",
  wikipedia: "Wikipedia", screenscraper: "ScreenScraper", igdb: "IGDB", adb: "Arcade Database",
};

// The name a game is shown by everywhere: the clean title from its
// parsed name, else the set's label, else the DAT title.
export const displayTitle = (r) => r?.name?.title || r?.label || r?.title || "";

// How long something ran: "45 s", "12 min", "3.2 h". `seconds: false`
// rounds anything short to at least "1 min" (recorded play time).
export function duration(s, { seconds = true } = {}) {
  if (seconds && s < 90) return `${s} s`;
  if (s < 5400) return `${Math.max(1, Math.round(s / 60))} min`;
  return `${(s / 3600).toFixed(1)} h`;
}

// What the name says, as one line: "USA · En, Ja · Rev 1 · Beta".
// `flags`: the regions are shown as flags, so leave them out.
export function edition(n, flags = false) {
  if (!n) return "";
  return [
    flags ? "" : n.regions.join(", "),
    n.languages.join(", "),
    n.version,
    n.status,
    n.disc && `Disc ${n.disc}`,
    n.translation && `translation: ${n.translation}`,
    n.hack && `hack: ${n.hack}`,
    n.bad_dump && "bad dump",
    n.alternate && "alternate dump",
    n.unlicensed && "unlicensed",
  ].filter(Boolean).join(" · ");
}

// A picture's caption: what it is, its size as loaded (`dims`: kind ->
// [width, height]) and where it came from.
export function caption(kind, dims, detail) {
  const d = dims[kind];
  const src = detail?.art_sources?.[kind];
  return [artLabel[kind], d ? `${d[0]}×${d[1]}` : "", src ? sourceLabel[src] ?? src : ""].filter(Boolean).join(" · ");
}

// 1991, 1991-08 or 1991-08-21.
export function released(f) {
  if (!f.releaseyear) return "";
  return [f.releaseyear, f.releasemonth, f.releaseday]
    .filter(Boolean)
    .map((v, i) => (i ? String(v).padStart(2, "0") : v))
    .join("-");
}

// Play time romforge recorded: "3.2 h · 5 times · last 19 Oct 2026".
export function played(p) {
  const time = duration(p.seconds, { seconds: false });
  const last = new Date(p.last_played * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  return `${time} · ${p.sessions === 1 ? "once" : `${p.sessions} times`} · last ${last}`;
}
