// A game's versions under their headings, as the list, the picture views
// and Details' Versions tab show them: other regions, editions,
// translations, hacks, and patch files apart (as translations and hacks).

const CATS = [
  ["region", "Other regions"],
  ["edition", "Editions"],
  ["translation", "Translations"],
  ["hack", "Hacks"],
];
const PATCH_CATS = [
  ["patch_translation", "Translations"],
  ["patch_hack", "Hacks"],
];

/// The lines for a group's versions (`others`: every row of the group but
/// its original), headings (`{ head, id, key, label, n, depth }`) first
/// and their rows after when `isOpen(key)`. Keys start with `group`.
export function versionLines(group, others, isOpen, depth = 1) {
  const out = [];
  const heading = (parent, cat, label, items, d) => {
    if (!items.length) return;
    const key = `${parent}|${cat}`;
    out.push({ head: true, id: `h:${key}`, key, label, n: items.length, depth: d });
    if (isOpen(key)) for (const m of items) out.push({ ...m, depth: d + 1 });
  };
  for (const [c, label] of CATS) heading(group, c, label, others.filter((m) => m.category === c), depth);
  const patches = others.filter((m) => m.category?.startsWith("patch_"));
  if (patches.length) {
    const key = `${group}|patches`;
    out.push({ head: true, id: `h:${key}`, key, label: "Patches", n: patches.length, depth });
    if (isOpen(key)) for (const [c, label] of PATCH_CATS) heading(key, c, label, patches.filter((m) => m.category === c), depth + 1);
  }
  // Anything without a heading (shouldn't happen) still shows.
  out.push(...others.filter((m) => !m.category).map((m) => ({ ...m, depth })));
  return out;
}
