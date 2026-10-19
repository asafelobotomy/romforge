// Sorting for every list: `sort` is { key, dir } (dir 1 ascending, -1
// descending); `getters` maps each key to the value it sorts by. Names
// compare as people read them ("Game 2" before "Game 10", case ignored);
// rows without a value go last either way.
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function sorted(items, sort, getters) {
  const get = getters[sort?.key];
  if (!get) return items;
  return [...items].sort((a, b) => {
    const x = get(a);
    const y = get(b);
    const xn = x === null || x === undefined || x === "";
    const yn = y === null || y === undefined || y === "";
    if (xn || yn) return xn === yn ? 0 : xn ? 1 : -1;
    const c = typeof x === "number" && typeof y === "number" ? x - y : collator.compare(String(x), String(y));
    return c * sort.dir;
  });
}

// The sort a list had last time (kept per list in this browser only).
export function remembered(name, fallback) {
  try {
    const s = JSON.parse(localStorage.getItem(`sort:${name}`) ?? "null");
    if (s && typeof s.key === "string" && (s.dir === 1 || s.dir === -1)) return s;
  } catch {}
  return fallback;
}

export function remember(name, sort) {
  try {
    localStorage.setItem(`sort:${name}`, JSON.stringify(sort));
  } catch {}
}
