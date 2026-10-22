// Moving the focus in the fullscreen mode with a pad or the keyboard. Pure
// functions over plain numbers, so they can be tested in Node: every
// screen there has a fixed shape (a carousel, a grid, a list), so no
// geometry is needed.
//
// Actions are the pad's (controller.rs): up, down, left, right, accept,
// back, north, west, menu (Start), context (Select), page_up / page_down
// (shoulders), first / last (triggers).

// Keys standing in for a pad's buttons.
const KEYS = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Enter: "accept",
  Escape: "back",
  Backspace: "back",
  Tab: "menu",
  " ": "context",
  o: "north",
  O: "north",
  f: "west",
  F: "west",
  PageUp: "page_up",
  PageDown: "page_down",
  Home: "first",
  End: "last",
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

// What an action means on the home screen, given where the focus is
// ("carousel" or "grid", which is the game list in the list layout:
// `layout: "list"` makes it one column). Returns { focus, platform, game } changes and,
// for the buttons, an `intent`: "open" (the game page), "menu" (Start),
// "context" (Select), "options" (North), "leave" (B on the home screen).
export function homeAction(state, action) {
  const { focus, platform, platforms, game, count, rows } = state;
  const cols = state.layout === "list" ? 1 : state.cols;
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
  const next = gridMove({ index: game, count, cols, rows }, action);
  return next === "out-up" ? { focus: "carousel" } : { game: next };
}
