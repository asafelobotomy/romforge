// The fullscreen mode's menus: Start's (main), Select's (context), a
// game's options (North), and questions. Each is { title, items: [{ label,
// run }], index }; `run` does the item and returns the next menu, or
// nothing to close it. `act` holds what the items do (from Fullscreen).

import { listLabel } from "../listLabel.js";
import { displayTitle } from "../labels.js";
import { themeChoices, themeList, withTheme } from "./theme.svelte.js";
import { detailsMenu } from "./fsdetails.js";

export function mainMenu(act, { sort, art }) {
  return {
    title: "romforge",
    index: 0,
    items: [
      { label: "Search", run: () => act.search() },
      ...(act.music() ? [{ label: "Music…", run: () => musicMenu(act, act.music()) }] : []),
      { label: "Back to desktop", run: () => act.leave() },
      ...(act.themes() ? [{ label: "Theme…", run: () => themeMenu(act, act.themes()) }] : []),
      { label: "Details…", run: () => detailsMenu(act, act.details()) },
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
  return { title: game ? displayTitle(game) : "Platform", index: 0, items };
}

// North on a game: play it, play another version, see it on the desktop.
export function optionsMenu(act, game, versions) {
  const items = [];
  if (game.playable) items.push({ label: "Play", run: () => act.play(game) });
  if (versions.length) items.push({ label: `Play another version (${versions.length})`, run: () => versionsMenu(act, game, versions) });
  if (game.playable) items.push({ label: "Emulator…", run: () => act.emulator(game) });
  items.push({ label: "Show in desktop", run: () => act.desktop(game) });
  return { title: displayTitle(game), index: 0, items };
}

export function versionsMenu(act, game, versions) {
  return {
    title: `${displayTitle(game)}: versions`,
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

const next = (list, v) => list[(list.indexOf(v) + 1) % list.length];
const labelOf = (list, v) => list.find((c) => c.value === v)?.label ?? v;

// Start → Theme: the theme, its colours, layout, text size and options.
// Pressing an item changes it at once (and it is kept); `index` stays on
// the item pressed. `view` is themes_view; `act.theme(selection)` keeps one.
export function themeMenu(act, view, index = 0) {
  const c = themeChoices(view);
  if (!c) return { title: "Theme", index: 0, items: [{ label: "No theme to show", run: () => undefined }] };
  const again = (change, i) => {
    const s = { ...view.selection, ...change };
    act.theme(s);
    return themeMenu(act, { ...view, selection: s }, i);
  };
  const values = (list) => list.map((x) => x.value);
  const items = [
    { label: `Theme: ${c.manifest.name}`, run: () => themesMenu(act, view) },
    { label: "Get themes…", run: () => act.store() },
    { label: `Colours: ${labelOf(c.schemes, c.scheme)}`, run: () => again({ scheme: next(values(c.schemes), c.scheme) }, 2) },
  ];
  if (c.layouts.length > 1) items.push({ label: `Layout: ${labelOf(c.layouts, c.layout)}`, run: () => again({ layout: next(values(c.layouts), c.layout) }, 3) });
  const sizeAt = items.length;
  items.push({ label: `Text size: ${c.textSize}%`, run: () => again({ text_size: next(c.textSizes, c.textSize) }, sizeAt) });
  for (const o of c.options) {
    const at = items.length;
    items.push({ label: `${o.label}: ${labelOf(o.choices, o.value)}`, run: () => again(c.withOption(o.key, next(values(o.choices), o.value)), at) });
  }
  return { title: "Theme", index, items };
}

// The themes to pick from (a refused one isn't listed here; Settings says why).
export function themesMenu(act, view) {
  const usable = themeList(view);
  return {
    title: "Pick a theme",
    index: Math.max(0, usable.findIndex((t) => t.id === view.selection.theme)),
    items: usable.map((t) => ({
      label: t.label,
      run: () => {
        const s = { ...view.selection, ...withTheme(t.id) };
        act.theme(s);
        return themeMenu(act, { ...view, selection: s });
      },
    })),
  };
}

// Start → Theme → Get themes: the community index (themes_available).
// A theme to get asks first, with its licence; one there is used.
export function storeMenu(act, available) {
  if (!available) return { title: "Get themes", index: 0, items: [{ label: "The list couldn't be fetched (offline?)", run: () => undefined }] };
  const state = (t) =>
    ({ not_installed: "", installed: " (installed)", update_available: " (update)", taken: " (name taken)", refused: " (can't install)" })[t.status.state] ?? "";
  const items = available.themes.map((t) => ({
    label: `${t.name} ${t.version}${state(t)}`,
    run: () => {
      const s = t.status.state;
      if (s === "installed") return ask(`Use ${t.name} now?`, () => act.useTheme(t.id));
      if (s !== "not_installed" && s !== "update_available") return undefined;
      return ask(`${t.name} by ${t.author || "unknown"}, licence ${t.licence}. Download it?`, () => act.install(t));
    },
  }));
  if (!items.length) items.push({ label: "No themes in the list yet", run: () => undefined });
  return { title: "Get themes", index: 0, items };
}

// The emulator a game plays in (stage 2, step 6): romforge's choice, or
// one of the installed ones for its platform, kept for that game.
export function emulatorMenu(act, game, { picked, choices }) {
  const items = [
    { label: `${picked ? "" : "✓ "}romforge's choice`, run: () => act.setEmulator(game, null) },
    ...choices.map((name) => ({ label: `${picked === name ? "✓ " : ""}${name.replace(/^retroarch:/, "RetroArch: ")}`, run: () => act.setEmulator(game, name) })),
  ];
  return { title: `${displayTitle(game)}: emulator`, index: Math.max(0, choices.indexOf(picked) + 1), items };
}

// Start → Music (stage 2, step 6): the track playing, next, pause, volume.
export function musicMenu(act, m, index = 0) {
  const again = (f, i) => (f(), musicMenu(act, act.music(), i));
  return {
    title: m.title ? `Music: ${m.title}` : "Music",
    index,
    items: [
      { label: "Next track", run: () => again(act.nextTrack, 0) },
      { label: m.playing ? "Pause" : "Play", run: () => again(act.toggleMusic, 1) },
      { label: `Volume: ${m.volume}% (louder)`, run: () => again(() => act.volume(m.volume >= 100 ? 0 : m.volume + 10), 2) },
    ],
  };
}
