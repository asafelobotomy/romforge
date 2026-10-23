// What the fullscreen mode shows of the game in focus, each detail on or
// off (the core's settings::FS_DETAILS, in its order): kept in Settings →
// Display and changed there or in Start → Details. Pure, so it is tested
// in Node.

export const DETAILS = [
  { key: "logo", label: "Logo (else title)" },
  { key: "platform", label: "Platform" },
  { key: "released", label: "Release date" },
  { key: "genre", label: "Genre" },
  { key: "players", label: "Players" },
  { key: "modes", label: "Modes" },
  { key: "developer", label: "Developer" },
  { key: "publisher", label: "Publisher" },
  { key: "rating", label: "Age rating" },
  { key: "community", label: "Community rating" },
  { key: "edition", label: "Region and edition" },
  { key: "derived", label: "Hack or translation of" },
  { key: "played", label: "Play time" },
  { key: "overview", label: "Overview" },
  { key: "backdrop", label: "Fanart backdrop" },
];

// Shown until the settings have loaded (as the core's defaults).
export const SHOWN = ["logo", "platform", "released", "genre", "players", "developer", "community", "derived", "played", "overview"];

// `shown` with `key` turned on or off, in DETAILS' order.
export function toggled(shown, key) {
  const on = new Set(shown);
  if (on.has(key)) on.delete(key);
  else on.add(key);
  return DETAILS.map((d) => d.key).filter((k) => on.has(k));
}

// Start → Details: each detail and whether it shows; pressing one turns
// it on or off at once (and it is kept); `index` stays on the item pressed.
export function detailsMenu(act, shown, index = 0) {
  return {
    title: "Details",
    index,
    items: DETAILS.map((d, i) => ({
      label: `${d.label}: ${shown.includes(d.key) ? "On" : "Off"}`,
      run: () => detailsMenu(act, act.setDetails(toggled(shown, d.key)), i),
    })),
  };
}
