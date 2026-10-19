import { sorted } from "./sort.js";

// The Series tab: each title once (the platforms you have it on
// together), numbered entries apart from the rest.
const ordNum = (o) => {
  const n = parseFloat(o);
  return Number.isFinite(n) ? n : null;
};
const yearNum = (y) => (y ? Number(String(y).slice(0, 4)) || null : null);
const seriesGetters = {
  order: (e) => ordNum(e.ordinal) ?? yearNum(e.year),
  year: (e) => yearNum(e.year),
  title: (e) => e.title,
  have: (e) => (e.mine.length ? 0 : 1),
};
export function seriesParts(list, seriesSort) {
  const by = new Map();
  for (const s of list) {
    const k = s.title.toLowerCase();
    let e = by.get(k);
    if (!e) {
      e = { title: s.title, ordinal: s.ordinal, year: s.year, current: false, mine: [] };
      by.set(k, e);
    }
    e.ordinal ??= s.ordinal;
    if (yearNum(s.year) && (!yearNum(e.year) || yearNum(s.year) < yearNum(e.year))) e.year = s.year;
    e.current ||= s.current;
    if (s.in_library) e.mine.push(s);
  }
  const all = [...by.values()];
  // Unnumbered games have no series order: by year then.
  const others = seriesSort.key === "order" ? { key: "year", dir: seriesSort.dir } : seriesSort;
  return {
    numbered: sorted(all.filter((e) => e.ordinal), seriesSort, seriesGetters),
    others: sorted(all.filter((e) => !e.ordinal), others, seriesGetters),
    total: all.length,
    have: all.filter((e) => e.mine.length).length,
  };
}
