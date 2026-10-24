import { invoke } from "@tauri-apps/api/core";

// The fullscreen mode's theme (stage 2, step 4): what the core says to
// apply (themes::Applied), loaded once and kept here. The theme's CSS is a
// <link> to the theme:// scheme (checked and scoped to [data-fs-root] by
// the core); its scheme's and options' variables and data-fs-opt-*
// attributes go on the root. While the mode is open, an edited user theme
// reloads within a second.
export const theme = $state({
  // themes::Applied, once loaded.
  applied: null,
  // themes_view: the themes, the pick, the text sizes, the folder.
  view: null,
  // Counts the theme's stylesheet loads (its CSS may set variables too).
  cssLoads: 0,
});

export async function loadThemes() {
  const v = await invoke("themes_view").catch(() => null);
  if (!v) return;
  theme.view = v;
  theme.applied = v.applied;
}

// Keep a new pick and apply it at once.
export async function pickTheme(selection) {
  if (theme.view) theme.view.selection = selection;
  const a = await invoke("theme_set", { selection }).catch(() => null);
  if (a) theme.applied = a;
  const v = await invoke("themes_view").catch(() => null);
  if (v) theme.view = v;
}

// The layouts by name, where their key as words isn't enough.
const LAYOUT_NAMES = { hero: "Hero row", "hero-bottom": "Hero row, bottom" };

// A scheme's or layout's key as words: "high-contrast" → "High contrast".
export const nice = (s) => s.charAt(0).toUpperCase() + s.slice(1).replaceAll("-", " ");

// The themes that can be picked (a refused one isn't listed), named, and
// the change to a selection that picks one (its own colours and layout).
export function themeList(view) {
  return (view?.themes ?? [])
    .filter((t) => t.manifest && !t.problem)
    .map((t) => ({ id: t.id, label: t.manifest.name + (t.builtin ? "" : t.downloaded ? " (downloaded)" : " (yours)") }));
}
export const withTheme = (id) => ({ theme: id, scheme: "", layout: "" });

// What a theme pick offers (`view`: themes_view), the same in Settings and
// the fullscreen mode's Start → Theme: the usable themes, the picked one's
// colours (with the system's first when it has light and dark), layouts,
// text sizes and options, each with its current value. `withOption`
// gives the change to a selection. Null without a manifest.
export function themeChoices(view) {
  const sel = view?.selection;
  const m = sel && view.themes.find((t) => t.id === sel.theme && t.manifest)?.manifest;
  if (!m) return null;
  const names = Object.keys(m.schemes);
  const follows = names.includes("light") && names.includes("dark");
  const mine = sel.options?.[sel.theme] ?? {};
  return {
    manifest: m,
    themes: themeList(view),
    schemes: [...(follows ? [{ value: "", label: "With the system" }] : []), ...names.map((s) => ({ value: s, label: nice(s) }))],
    scheme: sel.scheme || (follows ? "" : names[0]),
    layouts: [m.layout, ...(m.layouts ?? []).filter((l) => l !== m.layout)].map((l) => ({ value: l, label: LAYOUT_NAMES[l] ?? nice(l) })),
    layout: sel.layout || m.layout,
    textSizes: view.text_sizes,
    textSize: sel.text_size,
    options: (m.options ?? []).map((o) => ({ key: o.key, label: o.label, choices: o.choices, value: mine[o.key] ?? o.choices[0].value })),
    withOption: (key, value) => ({ options: { ...sel.options, [sel.theme]: { ...mine, [key]: value } } }),
  };
}

const LINK = "romforge-fs-theme";

// The theme's stylesheet in the page (one at a time), or none.
export function themeLink(applied) {
  let link = document.getElementById(LINK);
  if (!applied) return link?.remove();
  if (!link) {
    link = document.createElement("link");
    link.id = LINK;
    link.rel = "stylesheet";
    link.onload = () => theme.cssLoads++;
    document.head.appendChild(link);
  }
  const href = `theme://localhost/${encodeURIComponent(applied.theme)}/theme.css?v=${applied.stamp}`;
  if (link.getAttribute("href") !== href) link.setAttribute("href", href);
}

// The scheme `applied` shows (the system's light or dark when none is
// picked) and the variables its scheme and options set.
export function themeVars(applied, dark) {
  const scheme = applied.scheme ?? (dark ? "dark" : "light");
  return { scheme, vars: { ...(applied.schemes[scheme] ?? {}), ...applied.vars } };
}

// The variables and attributes for `applied` on `node` (the root), the
// scheme following the system's when the theme has light and dark.
// The card's shape (--fs-card-ratio) for the layouts that count what
// fits: as the theme's scheme or options set it, else as its CSS does on
// `el` (read again once that has loaded), else the default. Called in a
// $derived that also reads the space, so it is read again as that changes.
export function cardRatio(el, dark) {
  const set = theme.applied && parseFloat(themeVars(theme.applied, dark).vars["--fs-card-ratio"]);
  if (set) return set;
  void theme.cssLoads;
  return (el && parseFloat(getComputedStyle(el).getPropertyValue("--fs-card-ratio"))) || 0.74;
}

export function applyTheme(node, applied, dark) {
  for (const name of [...node.style]) if (name.startsWith("--fs-")) node.style.removeProperty(name);
  for (const a of [...node.attributes]) if (a.name.startsWith("data-fs-opt-")) node.removeAttribute(a.name);
  if (!applied) return;
  const { scheme, vars } = themeVars(applied, dark);
  for (const [k, v] of Object.entries(vars)) node.style.setProperty(k, v);
  node.style.setProperty("--fs-scale", String(applied.scale));
  for (const [k, v] of Object.entries(applied.attrs)) node.setAttribute(`data-fs-opt-${k}`, v);
  node.dataset.fsTheme = applied.theme;
  node.dataset.fsScheme = scheme;
  node.dataset.fsLayout = applied.layout;
}

// Poll the theme's files while the mode is open; a change reloads its CSS.
// Returns the stop function.
export function watchTheme() {
  const t = setInterval(async () => {
    const a = theme.applied;
    if (!a) return;
    const stamp = await invoke("theme_stamp", { id: a.theme }).catch(() => a.stamp);
    if (stamp !== a.stamp && theme.applied?.theme === a.theme) {
      // The manifest may have changed too (schemes, options).
      const v = await invoke("themes_view").catch(() => null);
      if (v) {
        theme.view = v;
        theme.applied = { ...v.applied, stamp };
      } else theme.applied = { ...a, stamp };
    }
  }, 1000);
  return () => clearInterval(t);
}


// The theme while the fullscreen mode is open (stage 2, step 4): its CSS
// linked, its variables and attributes on the root (`rootOf()`), the
// system's light or dark followed, and an edited user theme reloaded.
// Called while a component sets up; its `dark` follows the system's.
export function followTheme(rootOf) {
  const system = $state({ dark: window.matchMedia("(prefers-color-scheme: dark)").matches });
  loadThemes();
  $effect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const on = (e) => (system.dark = e.matches);
    mq.addEventListener("change", on);
    const stop = watchTheme();
    return () => {
      mq.removeEventListener("change", on);
      stop();
      themeLink(null);
    };
  });
  $effect(() => themeLink(theme.applied));
  $effect(() => {
    const root = rootOf();
    if (root) applyTheme(root, theme.applied, system.dark);
  });
  return system;
}
