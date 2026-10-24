// The help bar's items for what is on screen: [key or action, what it
// does]. Pure, so it is tested in Node.

export function helpBar({ menu, screen, buttons = [], button = 0, quitChord = "", boxOpen, focus, layout, fav }) {
  if (menu) return [["move", "Move"], ["accept", "Choose"], ["back", "Close"]];
  if (screen === "search") return [["move", "Move"], ["accept", "Type"], ["north", "Space"], ["back", "Delete"], ["menu", "Results"]];
  if (screen === "game") {
    const quit = quitChord ? [[quitChord, "Quits a game"]] : [];
    return [["accept", buttons[button]?.label ?? "Choose"], ["side", "Game"], ["west", "Favourite"], ["north", "Options"], ["back", "Back"], ...quit];
  }
  if (boxOpen) return [["side", "Turn"], ["accept", "Game"], ["north", "Options"], ["back", "Back"]];
  if (focus === "carousel") return [["side", "Platform"], ["accept", "Games"], ["back", "Desktop"], ["menu", "Menu"]];
  return [["accept", layout === "bookshelf" ? "Take out" : "Open"], ["west", fav ? "Unfavourite" : "Favourite"], ["north", "Options"], ["context", "More"], ["back", "Platforms"], ["menu", "Menu"]];
}
