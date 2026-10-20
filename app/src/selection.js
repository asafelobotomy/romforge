// The library's selection: the games the toolbar's jobs act on. `sel` is
// { picked: Set of row ids, anchor: where a Shift run starts, focus: the
// row Details shows }; every function returns a new one. Headings are
// never picked.

const ids = (rows) => rows.filter((r) => !r.head).map((r) => r.id);

// The rows from `a` to `b` (either end), as listed.
function run(rows, a, b) {
  const list = ids(rows);
  let i = list.indexOf(a);
  const j = list.indexOf(b);
  if (i < 0) i = j;
  return new Set(list.slice(Math.min(i, j), Math.max(i, j) + 1));
}

export const none = () => ({ picked: new Set(), anchor: null, focus: null });

// A click: alone it picks one row; Ctrl (⌘) adds or drops one; Shift
// picks the run from the anchor (Ctrl+Shift adds the run).
export function click(sel, rows, id, e) {
  const add = e?.ctrlKey || e?.metaKey;
  if (e?.shiftKey && sel.anchor != null) {
    const r = run(rows, sel.anchor, id);
    return { picked: add ? new Set([...sel.picked, ...r]) : r, anchor: sel.anchor, focus: id };
  }
  if (add) {
    const picked = new Set(sel.picked);
    if (!picked.delete(id)) picked.add(id);
    return { picked, anchor: id, focus: id };
  }
  return { picked: new Set([id]), anchor: id, focus: id };
}

// The keys: arrows, Home, End, Page Up/Down move (with Shift they pick
// the run, with Ctrl only the focus moves); Ctrl+A picks everything
// listed, Ctrl+Shift+A nothing, Ctrl+I the others, Ctrl+Space adds or
// drops the focused row. Null for any other key.
export function key(sel, rows, e) {
  const ctrl = e.ctrlKey || e.metaKey;
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const list = ids(rows);
  if (ctrl && k === "a") return { ...sel, picked: e.shiftKey ? new Set() : new Set(list) };
  if (ctrl && k === "i") return { ...sel, picked: new Set(list.filter((id) => !sel.picked.has(id))) };
  if (ctrl && k === " " && sel.focus != null) return click(sel, rows, sel.focus, { ctrlKey: true });
  const i = list.indexOf(sel.focus);
  const page = 10;
  const to = {
    ArrowDown: i < 0 ? 0 : i + 1,
    ArrowUp: i < 0 ? 0 : i - 1,
    PageDown: i + page,
    PageUp: i - page,
    Home: 0,
    End: list.length - 1,
  }[k];
  if (to === undefined || !list.length) return null;
  const id = list[Math.max(0, Math.min(list.length - 1, to))];
  if (e.shiftKey) {
    const anchor = sel.anchor ?? sel.focus ?? id;
    return { picked: run(rows, anchor, id), anchor, focus: id };
  }
  if (ctrl) return { ...sel, focus: id };
  return { picked: new Set([id]), anchor: id, focus: id };
}

// What the jobs get: the picked rows still listed (a filter or another
// platform hides the rest); a game whose versions are folded brings them
// along, since they can't be picked one by one.
export function targets(picked, rows, libRows, isOpen) {
  const out = new Set();
  for (const r of rows) {
    if (r.head || !picked.has(r.id)) continue;
    out.add(r.id);
    if (r.main && r.group && !isOpen(r.group)) for (const m of libRows) if (m.group === r.group) out.add(m.id);
  }
  return [...out];
}
