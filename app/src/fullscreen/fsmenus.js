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
      ...(act.themes() ? [{ label: "Theme…", run: () => themeMenu(act, act.themes()) }] : []),
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

const nice = (s) => s.charAt(0).toUpperCase() + s.slice(1).replaceAll("-", " ");
const next = (list, v) => list[(list.indexOf(v) + 1) % list.length];

// Start → Theme: the theme, its colours, layout, text size and options.
// Pressing an item changes it at once (and it is kept); `index` stays on
// the item pressed. `view` is themes_view; `act.theme(selection)` keeps one.
export function themeMenu(act, view, index = 0) {
  const sel = view.selection;
  const m = view.themes.find((t) => t.id === sel.theme && t.manifest)?.manifest;
  if (!m) return { title: "Theme", index: 0, items: [{ label: "No theme to show", run: () => undefined }] };
  const again = (change, i) => {
    const s = { ...sel, ...change };
    act.theme(s);
    return themeMenu(act, { ...view, selection: s }, i);
  };
  const schemes = Object.keys(m.schemes);
  const follows = schemes.includes("light") && schemes.includes("dark");
  const schemeList = follows ? ["", ...schemes] : schemes;
  const scheme = sel.scheme || (follows ? "" : schemes[0]);
  const layouts = [m.layout, ...(m.layouts ?? []).filter((l) => l !== m.layout)];
  const layout = sel.layout || m.layout;
  const mine = sel.options?.[sel.theme] ?? {};
  const items = [
    { label: `Theme: ${m.name}`, run: () => themesMenu(act, view) },
    { label: "Get themes…", run: () => act.store() },
    {
      label: `Colours: ${scheme ? nice(scheme) : "with the system"}`,
      run: () => again({ scheme: next(schemeList, scheme) }, 2),
    },
  ];
  if (layouts.length > 1) items.push({ label: `Layout: ${nice(layout)}`, run: () => again({ layout: next(layouts, layout) }, 3) });
  const sizeAt = items.length;
  items.push({
    label: `Text size: ${sel.text_size}%`,
    run: () => again({ text_size: next(view.text_sizes, sel.text_size) }, sizeAt),
  });
  for (const o of m.options ?? []) {
    const values = o.choices.map((c) => c.value);
    const v = mine[o.key] ?? values[0];
    const label = o.choices.find((c) => c.value === v)?.label ?? v;
    const at = items.length;
    items.push({
      label: `${o.label}: ${label}`,
      run: () => again({ options: { ...sel.options, [sel.theme]: { ...mine, [o.key]: next(values, v) } } }, at),
    });
  }
  return { title: "Theme", index, items };
}

// The themes to pick from (a refused one isn't listed here; Settings says why).
export function themesMenu(act, view) {
  const usable = view.themes.filter((t) => t.manifest && !t.problem);
  return {
    title: "Pick a theme",
    index: Math.max(0, usable.findIndex((t) => t.id === view.selection.theme)),
    items: usable.map((t) => ({
      label: t.manifest.name + (t.builtin ? "" : t.downloaded ? " (downloaded)" : " (yours)"),
      run: () => {
        const s = { ...view.selection, theme: t.id, scheme: "", layout: "" };
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
