// A game pad on the desktop library: the list moves with the d-pad, A
// plays, the shoulders switch platform, the triggers jump to the first or
// last game, Start opens Settings and Select Emulator Settings. `d` holds
// what App knows and does (its state is passed in, not imported).
export function desktopPad(action, d) {
  if (d.view !== "library") return;
  const games = d.rows.filter((r) => !r.head);
  if (action === "up" || action === "down") d.listKeys({ key: action === "up" ? "ArrowUp" : "ArrowDown", preventDefault() {} });
  else if (action === "accept" && d.row?.playable && !d.busy) d.play();
  else if (action === "page_up" || action === "page_down") {
    const all = ["*", ...d.platforms.map((p) => p.system ?? "")];
    const i = Math.max(0, all.indexOf(d.platform));
    d.setPlatform(all[(i + (action === "page_down" ? 1 : all.length - 1)) % all.length]);
  } else if ((action === "first" || action === "last") && games.length) {
    d.pick((action === "first" ? games[0] : games[games.length - 1]).id);
  } else if (action === "menu") d.settings.openSettings();
  else if (action === "context") d.emulators.openEmulatorSettings();
}
