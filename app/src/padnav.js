// Game pads in the desktop UI (milestone 18, stage 2, step 1). The core
// reads the pads (gilrs) and sends "pad" events; this turns their actions
// into what the keyboard and mouse already do. In an open dialog the
// d-pad moves the focus, A presses what has it and B closes the dialog;
// elsewhere `library` gets the action.

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/// Move the focus within `root` by `step` (±1), wrapping around.
export function moveFocus(root, step) {
  const els = [...root.querySelectorAll(FOCUSABLE)].filter((e) => e.offsetParent !== null);
  if (!els.length) return;
  const i = els.indexOf(document.activeElement);
  const j = i < 0 ? (step > 0 ? 0 : els.length - 1) : (i + step + els.length) % els.length;
  els[j].focus();
  els[j].scrollIntoView({ block: "nearest" });
}

/// The dialog on top, if one is open.
function topDialog() {
  const all = [...document.querySelectorAll('[role="dialog"]')];
  return all[all.length - 1] ?? null;
}

/// Do `action` (a pad's) in the open dialog, else hand it to `library`.
export function padAction(action, library) {
  const dialog = topDialog();
  if (!dialog) return library(action);
  if (action === "up" || action === "left") moveFocus(dialog, -1);
  else if (action === "down" || action === "right") moveFocus(dialog, 1);
  else if (action === "accept") {
    const el = document.activeElement;
    if (el && dialog.contains(el)) el.click();
    else moveFocus(dialog, 1);
  } else if (action === "back") window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
}
