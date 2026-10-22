// The fullscreen mode's on-screen keyboard (stage 2, step 6): rows of
// keys moved through with the d-pad. Pure, so it is tested in Node. Up
// and down keep to the same place across rows of different lengths;
// left and right wrap within a row.

import { displayTitle } from "../labels.js";

export const ROWS = [
  ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"],
  ["K", "L", "M", "N", "O", "P", "Q", "R", "S", "T"],
  ["U", "V", "W", "X", "Y", "Z", "1", "2", "3", "4"],
  ["5", "6", "7", "8", "9", "0", "-", "'", "&", ":"],
  ["space", "delete", "clear", "done"],
];

// What a key types or does: a character, or "delete", "clear", "done".
export const LABELS = { space: "Space", delete: "⌫", clear: "Clear", done: "Results" };

// Where the focus goes from `{ row, col }` on `action`.
export function keyMove({ row, col }, action) {
  const width = (r) => ROWS[r].length;
  if (action === "left") return { row, col: (col - 1 + width(row)) % width(row) };
  if (action === "right") return { row, col: (col + 1) % width(row) };
  if (action === "up" || action === "down") {
    const to = action === "up" ? (row - 1 + ROWS.length) % ROWS.length : (row + 1) % ROWS.length;
    // The same place across: by the middle of the key.
    const at = (col + 0.5) / width(row);
    return { row: to, col: Math.min(width(to) - 1, Math.floor(at * width(to))) };
  }
  return { row, col };
}

export const keyAt = ({ row, col }) => ROWS[row][col];

// The query after pressing `key`.
export function typed(query, key) {
  if (key === "delete") return query.slice(0, -1);
  if (key === "clear") return "";
  if (key === "done") return query;
  return (query + (key === "space" ? " " : key.toLowerCase())).slice(0, 40);
}

// Games whose title (or a version's) has every word of `query`, the ones
// starting with it first.
export function search(all, query, limit = 40) {
  const words = norm(query).split(" ").filter(Boolean);
  if (!words.length) return [];
  const hits = all.filter((r) => {
    const t = norm(`${r.name?.title ?? ""} ${r.title}`);
    return words.every((w) => t.includes(w));
  });
  const first = norm(query);
  const starts = (r) => (norm(displayTitle(r)).startsWith(first) ? 0 : 1);
  return hits.sort((a, b) => starts(a) - starts(b) || norm(displayTitle(a)).localeCompare(norm(displayTitle(b)))).slice(0, limit);
}

const norm = (s) =>
  s
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9&']+/g, " ")
    .trim();
