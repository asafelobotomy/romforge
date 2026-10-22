import { getCurrentWindow } from "@tauri-apps/api/window";

// The fullscreen mode (stage 2): whether it is on, and what the pad and
// keys go to while it is (the mode's own handler, set by Fullscreen).
export const fs = $state({
  on: false,
  // The mode's handler for a pad action; null while it isn't mounted.
  pad: null,
  // Whether a pad was seen (the help bar shows its buttons, else keys).
  padSeen: false,
});

async function windowFullscreen(on) {
  try {
    await getCurrentWindow().setFullscreen(on);
  } catch {
    // Not allowed or not supported: the mode still covers the window.
  }
}

export function enter() {
  // A desktop field keeping the focus would take the keys (the filter,
  // hidden behind the mode).
  document.activeElement?.blur?.();
  fs.on = true;
  windowFullscreen(true);
}

export function leave() {
  fs.on = false;
  windowFullscreen(false);
}

export function toggle() {
  if (fs.on) leave();
  else enter();
}

// Remembered between runs: the last platform and game, the art kind, the
// sort. Wrapped: storage may be unavailable.
const KEY = "romforge.fullscreen";
export function remembered() {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}
export function remember(v) {
  try {
    localStorage.setItem(KEY, JSON.stringify(v));
  } catch {
    // Nothing kept this time.
  }
}
