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
});
