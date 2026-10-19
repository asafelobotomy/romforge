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
};
// Box art, fanart and logo have their own places; the rest sit under Media.
export const mediaKinds = ["box3d", "title", "snap", "icon"];
export const sourceLabel = { libretro: "libretro", screenscraper: "ScreenScraper", steamgriddb: "SteamGridDB", igdb: "IGDB", launchbox: "LaunchBox" };
export const extdbLabel = { openvgdb: "OpenVGDB", launchbox: "LaunchBox" };

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
