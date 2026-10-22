import { invoke } from "@tauri-apps/api/core";
import { gameKey } from "./fshome.js";

// Favourites and play time (stage 2, step 6), for the fullscreen mode's
// Favourites, Recently played and Most played: loaded when the mode opens
// and again after a game ends.
export const marks = $state({ favourites: new Set(), played: new Map() });

export async function loadMarks() {
  const m = await invoke("marks").catch(() => null);
  if (!m) return;
  marks.favourites = new Set(m.favourites.map(([system, title]) => gameKey({ system, title })));
  marks.played = new Map(m.played.map((p) => [gameKey(p), p]));
}

export const isFavourite = (r) => !!r && marks.favourites.has(gameKey(r));

// Favourite on or off, kept at once.
export async function toggleFavourite(r) {
  if (!r?.system) return;
  const on = !isFavourite(r);
  const next = new Set(marks.favourites);
  if (on) next.add(gameKey(r));
  else next.delete(gameKey(r));
  marks.favourites = next;
  await invoke("set_favourite", { system: r.system, title: r.title, on }).catch(() => loadMarks());
}
