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

const LINK = "romforge-fs-theme";

// The theme's stylesheet in the page (one at a time), or none.
export function themeLink(applied) {
  let link = document.getElementById(LINK);
  if (!applied) return link?.remove();
  if (!link) {
    link = document.createElement("link");
    link.id = LINK;
    link.rel = "stylesheet";
    document.head.appendChild(link);
  }
  const href = `theme://localhost/${encodeURIComponent(applied.theme)}/theme.css?v=${applied.stamp}`;
  if (link.getAttribute("href") !== href) link.setAttribute("href", href);
}

// The variables and attributes for `applied` on `node` (the root), the
// scheme following the system's when the theme has light and dark.
export function applyTheme(node, applied, dark) {
  for (const name of [...node.style]) if (name.startsWith("--fs-")) node.style.removeProperty(name);
  for (const a of [...node.attributes]) if (a.name.startsWith("data-fs-opt-")) node.removeAttribute(a.name);
  if (!applied) return;
  const scheme = applied.scheme ?? (dark ? "dark" : "light");
  const vars = { ...(applied.schemes[scheme] ?? {}), ...applied.vars };
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

