// The radial ring's icons, one per action (`romforge_core::hotkeys::Action`,
// snake_case): stroke paths on a 24x24 grid, drawn for romforge.
export const ICONS = {
  quit: "M12 3v8 M6.4 6.4a8 8 0 1 0 11.2 0",
  save_state: "M5 4h11l3 3v13H5z M8 4v5h7V4 M8 20v-6h8v6",
  load_state: "M12 4v10 M8 10l4 4 4-4 M5 19h14",
  menu: "M5 7h14 M5 12h14 M5 17h14",
  pause: "M8 5v14 M16 5v14",
  reset: "M4 12a8 8 0 1 0 2.5-5.8 M4 4v4.5h4.5",
  screenshot: "M4 8h3l2-3h6l2 3h3v11H4z M8.5 13.5a3.5 3.5 0 1 0 7 0 3.5 3.5 0 1 0-7 0",
  mute: "M4 9h4l5-4v14l-5-4H4z M17 9l4 6 M21 9l-4 6",
};

/// The path for an action, none for one without an icon.
export const iconFor = (action) => ICONS[action] ?? null;
