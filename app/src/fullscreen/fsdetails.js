// What the fullscreen mode shows of the game in focus, each detail on or
// off: kept in Settings → Display and changed there or in Start →
// Details. The details, named and in order, and those shown by default
// are the core's (settings::FS_DETAILS, sent by settings_info:
// knowDetails); none until then. Pure, so it is tested in Node.

export let DETAILS = [];
export let SHOWN = [];

// settings_info's fs_details: { details: [{ value, label }], shown }.
export function knowDetails(info) {
  if (!info?.details) return;
  DETAILS = info.details.map((d) => ({ key: d.value, label: d.label }));
  SHOWN = info.shown ?? [];
}

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
