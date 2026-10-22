// The fullscreen mode's attract mode and screensaver (stage 2, step 6),
// both off unless Settings turns them on: after a while with no input,
// games are shown in turn; later the screen dims. Any input wakes it, and
// that press does nothing else.
export const idle = $state({ mode: null });

let times = { attract: 0, saver: 0 };
let timer = null;
let paused = false;

function arm() {
  clearTimeout(timer);
  if (paused || !times.attract) return;
  timer = setTimeout(() => {
    idle.mode = "attract";
    if (times.saver) timer = setTimeout(() => (idle.mode = "saver"), times.saver);
  }, times.attract);
}

// Milliseconds before attract mode and then before the screensaver (0:
// never).
export function setIdle(attract, saver) {
  times = { attract, saver };
  arm();
}

// Input: whether it only woke the screen (then it is used up).
export function poke() {
  const woke = idle.mode !== null;
  idle.mode = null;
  arm();
  return woke;
}

// No attract mode while a game runs or the mode is closed.
export function pauseIdle(on) {
  paused = on;
  if (on) {
    clearTimeout(timer);
    idle.mode = null;
  } else arm();
}
