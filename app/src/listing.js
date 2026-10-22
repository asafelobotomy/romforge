import { sorted } from "./sort.js";
import { versionLines } from "./versions.js";
import { displayTitle } from "./labels.js";

// How the library is shown: a list, or cards of box art, 3D boxes or
// banners (kept for next time).
export const VIEWS = [
  ["list", "List", "☰"],
  ["boxart", "Box art", "▯"],
  ["box3d", "3D box", "◩"],
  ["banner", "Banner", "▬"],
];

// The view a list had last time (`name`: "library" or "versions").
export function storedView(name) {
  try {
    const v = localStorage.getItem(`view:${name}`);
    return VIEWS.some(([k]) => k === v) ? v : "list";
  } catch {
    return "list";
  }
}

export function storeView(name, mode) {
  try {
    localStorage.setItem(`view:${name}`, mode);
  } catch {}
}

// The list's sort (games by it, and the versions under each heading).
const statusOrder = ["verified", "incomplete", "by_you", "derivative", "detected", "candidate", "duplicate", "wrong_system", "missing", "patch", "unknown"];
const listGetters = {
  title: displayTitle,
  platform: (r) => r.system_name,
  year: (r) => r.year,
  status: (r) => statusOrder.indexOf(r.status),
};

// One row per game: its original (listed even when you only have
// translations, hacks or patches of it), expanding to its other versions
// under headings (regions, editions, translations, hacks, and patch
// files apart), each heading folding on its own. A group is listed when
// any of its versions matches the filter.
export function libraryRows(libRows, platform, filter, listSort, isOpen) {
  const match = (r) =>
    // All games and each platform: games only; Unidentified: whatever
    // isn't matched yet (unknown files, CRC-only candidates, other
    // platforms' matches, patches for no game), whatever its platform.
    (platform === "*" ? !r.unmatched : platform === "" ? r.unmatched : r.system === platform && !r.unmatched) &&
    (!filter ||
      r.title.toLowerCase().includes(filter.toLowerCase()) ||
      (r.name?.title || "").toLowerCase().includes(filter.toLowerCase()));
  const all = sorted(libRows, listSort, listGetters);
  const members = new Map();
  for (const r of all) if (r.group) members.set(r.group, [...(members.get(r.group) ?? []), r]);
  const out = [];
  for (const r of all) {
    if (!r.main) continue;
    const group = r.group ? members.get(r.group) : [r];
    if (!group.some(match)) continue;
    out.push(r);
    if (r.group && isOpen(r.group)) out.push(...versionLines(r.group, group.filter((m) => !m.main), isOpen));
  }
  return out;
}

// Details' Versions tab: the shown game's whole group, its original
// first.
export function versionRows(r, libRows, listSort, isOpen) {
  if (!r?.group) return r ? [{ ...r, versions: 1 }] : [];
  const group = sorted(
    libRows.filter((m) => m.group === r.group),
    listSort,
    listGetters,
  );
  const main = group.find((m) => m.main) ?? r;
  // The original here never folds the library's group.
  return [{ ...main, versions: 1, depth: 0 }, ...versionLines(r.group, group.filter((m) => m !== main), isOpen)];
}
