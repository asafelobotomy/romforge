// The fullscreen mode's home lists (stage 2, step 6): the platforms along
// the top, with All games and, when they have games, Favourites, Recently
// played and Most played first; and the games of whichever is picked.
// Pure, so it is tested in Node. A game is keyed by its system and title,
// as favourites and play time are kept.

import { displayTitle } from "../labels.js";

export const gameKey = (r) => `${r.system}\t${r.title}`;

// marks: { favourites: Set of keys, played: Map key → { seconds, sessions, last_played } }
export function virtualPlatforms(all, marks) {
  const n = (f) => all.filter(f).length;
  const fav = n((r) => marks.favourites.has(gameKey(r)));
  const played = n((r) => marks.played.has(gameKey(r)));
  return [
    { system: "*", name: "All games", short: "ALL", games: all.length },
    ...(fav ? [{ system: "*fav", name: "Favourites", short: "★", games: fav }] : []),
    ...(played ? [{ system: "*recent", name: "Recently played", short: "RECENT", games: Math.min(played, RECENT) }] : []),
    ...(played ? [{ system: "*most", name: "Most played", short: "MOST", games: Math.min(played, RECENT) }] : []),
  ];
}

// How many Recently played and Most played list.
export const RECENT = 50;

const titleOf = (r) => displayTitle(r).toLowerCase();

// The games of platform `key`, in `sort` order ("title" or "year"); the
// history lists keep their own order (latest, or longest played, first).
export function gamesFor(key, all, marks, sort) {
  const played = (r) => marks.played.get(gameKey(r));
  if (key === "*recent" || key === "*most") {
    const by = key === "*recent" ? (p) => p.last_played : (p) => p.seconds;
    return all
      .filter(played)
      .sort((a, b) => by(played(b)) - by(played(a)) || titleOf(a).localeCompare(titleOf(b)))
      .slice(0, RECENT);
  }
  return all
    .filter((r) => key === "*" || (key === "*fav" ? marks.favourites.has(gameKey(r)) : r.system === key))
    .sort((a, b) => (sort === "year" ? (a.year ?? 9999) - (b.year ?? 9999) : 0) || titleOf(a).localeCompare(titleOf(b)));
}
