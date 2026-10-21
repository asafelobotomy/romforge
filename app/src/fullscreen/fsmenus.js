// The fullscreen mode's menus: Start's (main), Select's (context), a
// game's options (North), and questions. Each is { title, items: [{ label,
// run }], index }; `run` does the item and returns the next menu, or
// nothing to close it. `act` holds what the items do (from Fullscreen).

import { listLabel } from "../listLabel.js";

export function mainMenu(act, { sort, art }) {
  return {
    title: "romforge",
    index: 0,
    items: [
      { label: "Back to desktop", run: () => act.leave() },
      { label: `Sort: ${sort === "title" ? "Title" : "Year"} (change)`, run: () => act.sort(sort === "title" ? "year" : "title") },
      { label: `Art: ${art === "box3d" ? "3D box" : "Box"} (change)`, run: () => act.art(art === "box3d" ? "boxart" : "box3d") },
      { label: "Quit romforge", run: () => ask("Quit romforge?", () => act.quit()) },
    ],
  };
}

// Select: what can be done with the focused game, else the platform.
export function contextMenu(act, game, versions) {
  const items = [];
  if (game && versions.length) items.push({ label: `Versions (${versions.length})`, run: () => versionsMenu(act, game, versions) });
  if (game) items.push({ label: "Show in desktop", run: () => act.desktop(game) });
  items.push({ label: "Back to desktop", run: () => act.leave() });
  return { title: game ? game.name?.title || game.title : "Platform", index: 0, items };
}

// North on a game: play it, play another version, see it on the desktop.
export function optionsMenu(act, game, versions) {
  const items = [];
  if (game.playable) items.push({ label: "Play", run: () => act.play(game) });
  if (versions.length) items.push({ label: `Play another version (${versions.length})`, run: () => versionsMenu(act, game, versions) });
  items.push({ label: "Show in desktop", run: () => act.desktop(game) });
  return { title: game.name?.title || game.title, index: 0, items };
}

export function versionsMenu(act, game, versions) {
  return {
    title: `${game.name?.title || game.title}: versions`,
    index: 0,
    items: versions.map((v) => ({
      label: `${listLabel(v)}${v.playable ? "" : " (not playable)"}`,
      run: () => (v.playable ? act.play(v) : undefined),
    })),
  };
}

export function ask(title, yes) {
  return {
    title,
    index: 1,
    items: [
      { label: "Yes", run: yes },
      { label: "No", run: () => undefined },
    ],
  };
}
