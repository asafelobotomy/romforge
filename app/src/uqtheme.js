// uQ's theme (uq-tokens) in the fullscreen mode. The window itself takes
// uQ's colours whenever they apply (app.css, [data-uq-variant]): it has no
// theme setting of its own and follows the system, which uQ is. The
// fullscreen mode has themes; uQ's colours stand in only for the default
// theme with Colours "With the system" (the core's `applied.desktop`). A
// theme or colours picked by hand keep their own.

// The default theme's colour variables, from uQ's (each falls back to
// romforge's own through vendor/uq-tokens/uq-tokens.css).
export const FS_FROM_UQ = Object.freeze({
  "--fs-bg": "var(--uq-bg)",
  "--fs-bg-2": "var(--uq-surface)",
  "--fs-panel": "var(--uq-surface)",
  "--fs-text": "var(--uq-text)",
  "--fs-muted": "var(--uq-text-muted)",
  "--fs-accent": "var(--uq-accent)",
  "--fs-accent-text": "var(--uq-accent-text)",
  "--fs-line": "var(--uq-border)",
  "--fs-focus": "var(--uq-accent)",
});

// The scheme `applied` (themes::Applied) shows and the variables it sets:
// the system's light or dark when none is picked (`dark`), uQ's variant
// and colours instead when uQ's tokens apply (`variant`: "dark", "light"
// or null) and the theme is the desktop's.
export function fsVars(applied, dark, variant = null) {
  const uq = Boolean(applied.desktop) && (variant === "dark" || variant === "light");
  const scheme = applied.scheme ?? ((uq ? variant === "dark" : dark) ? "dark" : "light");
  return {
    scheme,
    vars: { ...(applied.schemes[scheme] ?? {}), ...(uq ? FS_FROM_UQ : {}), ...applied.vars },
  };
}
