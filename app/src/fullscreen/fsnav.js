// Moving the focus in the fullscreen mode with a pad or the keyboard. Pure
// functions over plain numbers, so they can be tested in Node: every
// screen there has a fixed shape (a carousel, a grid, a list), so no
// geometry is needed.
//
// Actions are the pad's (controller.rs): up, down, left, right, accept,
// back, north, west, menu (Start), context (Select), page_up / page_down
// (shoulders), first / last (triggers).

// Keys standing in for a pad's buttons: each action's keys, the first
// being the one the help bar names (KEY_LABELS).
const BINDINGS = {
  up: ["ArrowUp"],
  down: ["ArrowDown"],
  left: ["ArrowLeft"],
  right: ["ArrowRight"],
  accept: ["Enter"],
  back: ["Escape", "Backspace"],
  menu: ["Tab"],
  context: [" "],
  north: ["o", "O"],
  west: ["f", "F"],
  page_up: ["PageUp"],
  page_down: ["PageDown"],
  first: ["Home"],
  last: ["End"],
};
const KEYS = Object.fromEntries(Object.entries(BINDINGS).flatMap(([action, keys]) => keys.map((k) => [k, action])));

// How the help bar names the keys: each action's, and the groups it shows
// as one (page, move, side).
const NAMES = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "◀", ArrowRight: "▶", Escape: "Esc", " ": "Space", o: "O", f: "F", PageUp: "PgUp", PageDown: "PgDn" };
const named = (action) => NAMES[BINDINGS[action][0]] ?? BINDINGS[action][0];
export const KEY_LABELS = {
  ...Object.fromEntries(Object.keys(BINDINGS).map((a) => [a, named(a)])),
  page: `${named("page_up")} / ${named("page_down")}`,
  move: "Arrows",
  side: `${named("left")} ${named("right")}`,
};

// The action a key press stands for, or null.
export function keyAction(key) {
  return KEYS[key] ?? null;
}

const clamp = (n, lo, hi) => Math.max(lo, Math.min(hi, n));

// A carousel (the platforms): left/right move, wrapping round.
export function carouselMove(index, count, action) {
  if (count <= 0) return 0;
  if (action === "left") return (index - 1 + count) % count;
  if (action === "right") return (index + 1) % count;
  if (action === "first") return 0;
  if (action === "last") return count - 1;
  return index;
}

// A list (a menu, the game page's buttons): up/down (and left/right)
// move, wrapping round.
export function listMove(index, count, action) {
  if (count <= 0) return 0;
  if (action === "up" || action === "left") return (index - 1 + count) % count;
  if (action === "down" || action === "right") return (index + 1) % count;
  if (action === "first") return 0;
  if (action === "last") return count - 1;
  return index;
}

// A grid of `count` cards in rows of `cols`, `rows` of them in view.
// Returns the new index, or "out-up" when up leaves the top row (the
// carousel takes the focus). Left/right go on to the row before or after;
// the shoulders move a page of rows; the triggers go to the first or the
// last card.
export function gridMove({ index, count, cols, rows = 3 }, action) {
  if (count <= 0) return action === "up" ? "out-up" : 0;
  cols = Math.max(1, cols);
  const last = count - 1;
  const i = clamp(index, 0, last);
  switch (action) {
    case "left":
      return Math.max(0, i - 1);
    case "right":
      return Math.min(last, i + 1);
    case "up":
      return i < cols ? "out-up" : i - cols;
    case "down":
      // Onto the last row even when the card right below isn't there.
      return i + cols <= last ? i + cols : Math.floor(i / cols) < Math.floor(last / cols) ? last : i;
    case "page_up":
      return Math.max(0, i - cols * Math.max(1, rows));
    case "page_down":
      return Math.min(last, i + cols * Math.max(1, rows));
    case "first":
      return 0;
    case "last":
      return last;
    default:
      return i;
  }
}

// The layouts whose games are one column (up and down move, up from the
// first goes to the carousel) or one row (left and right move, up goes to
// the carousel, down stays); the grid has `cols`, and shelves and the
// bookshelf are rows of their own (shelfMove).
const COLUMN = ["list", "wheel"];
const ROW = ["cover-flow", "hero", "hero-bottom"];

// Shelves: rows of games (`lengths`, each row's count), the focus at
// `row` and `col`. Returns the new { row, col }, or "out-up" when up
// leaves the first row. Up and down keep the column where the row is long
// enough; the shoulders page along the row by `page`, the triggers go to
// its ends. With `wrap` (the bookcase), right off a row's end goes on to
// the next row's first, left off its start to the previous row's last.
export function shelfMove({ row, col, lengths, page = 6, wrap = false }, action) {
  if (!lengths.length) return action === "up" ? "out-up" : { row: 0, col: 0 };
  const r = clamp(row, 0, lengths.length - 1);
  const last = (k) => Math.max(0, lengths[k] - 1);
  const c = clamp(col, 0, last(r));
  switch (action) {
    case "left":
      if (wrap && c === 0 && r > 0) return { row: r - 1, col: last(r - 1) };
      return { row: r, col: Math.max(0, c - 1) };
    case "right":
      if (wrap && c === last(r) && r < lengths.length - 1) return { row: r + 1, col: 0 };
      return { row: r, col: Math.min(last(r), c + 1) };
    case "up":
      return r === 0 ? "out-up" : { row: r - 1, col: Math.min(c, last(r - 1)) };
    case "down":
      return r === lengths.length - 1 ? { row: r, col: c } : { row: r + 1, col: Math.min(c, last(r + 1)) };
    case "page_up":
      return { row: r, col: Math.max(0, c - Math.max(1, page)) };
    case "page_down":
      return { row: r, col: Math.min(last(r), c + Math.max(1, page)) };
    case "first":
      return { row: r, col: 0 };
    case "last":
      return { row: r, col: last(r) };
    default:
      return { row: r, col: c };
  }
}

// What an action means on the home screen, given where the focus is
// ("carousel" or "grid", which is the games in any layout). `layout`
// shapes the moves (COLUMN, ROW, the grid's `cols`, or shelves' `shelf`:
// { row, col, lengths }). Returns { focus, platform, game, shelf } changes
// and, for the buttons, an `intent`: "open" (the game page), "menu"
// (Start), "context" (Select), "options" (North), "leave" (B on the home
// screen).
export function homeAction(state, action) {
  const { focus, platform, platforms, game, count, rows, layout } = state;
  const cols = COLUMN.includes(layout) || ROW.includes(layout) ? 1 : state.cols;
  if (action === "menu") return { intent: "menu" };
  if (action === "context") return { intent: "context" };
  if (focus === "carousel") {
    if (action === "left" || action === "right") {
      return { platform: carouselMove(platform, platforms, action), game: 0 };
    }
    if (action === "down" || action === "accept") return count ? { focus: "grid" } : {};
    if (action === "back") return { intent: "leave" };
    return {};
  }
  if (action === "accept") return count ? { intent: "open" } : {};
  if (action === "north") return count ? { intent: "options" } : {};
  if (action === "back") return { focus: "carousel" };
  if (layout === "shelves" || layout === "bookshelf") {
    const next = shelfMove({ ...state.shelf, page: rows }, action);
    return next === "out-up" ? { focus: "carousel" } : { shelf: next };
  }
  if (ROW.includes(layout) && (action === "up" || action === "down")) return action === "up" ? { focus: "carousel" } : {};
  const next = gridMove({ index: game, count, cols, rows }, action);
  return next === "out-up" ? { focus: "carousel" } : { game: next };
}
