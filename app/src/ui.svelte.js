import { remembered } from "./sort.js";
import { storedView } from "./listing.js";

// What Details keeps while you move between games, tabs and screens.
export const ui = $state({
  tab: "details",
  // kind -> [width, height] of the image as loaded (its real resolution).
  dims: {},
  // The image open at full size, if any.
  viewer: null,
  // The Versions tab: every heading open to start with; its own view
  // (list or pictures, kept for next time).
  vexpanded: {},
  versionsMode: storedView("versions"),
  // The Series tab's sort.
  seriesSort: remembered("series", { key: "order", dir: 1 }),
  // BIOS files to plan an import of on the BIOS page ("Import to BIOS").
  biosPaths: null,
  // A game is running (video snaps pause), and when the last one ended
  // (a pad press in the moment after is the game's, not romforge's).
  playing: false,
  returned: 0,
});

/// A pad action that arrives just after a game ends belongs to the game
/// (a held button, the press that quit it): romforge ignores it.
export const justBack = () => Date.now() - ui.returned < 700;
