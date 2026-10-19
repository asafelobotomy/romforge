// romforge's logos (its own artwork): the one the toolbar and the first-run
// screen show is picked in Settings → Display.
import synthwave from "./assets/logos/synthwave.png";
import pastel from "./assets/logos/pastel.png";
import gummy from "./assets/logos/gummy.png";
import space from "./assets/logos/space.png";
import sprinkles from "./assets/logos/sprinkles.png";

export const LOGOS = [
  ["synthwave", "Synthwave", synthwave],
  ["pastel", "Pastel", pastel],
  ["gummy", "Gummy", gummy],
  ["space", "Space", space],
  ["sprinkles", "Sprinkles", sprinkles],
];

/// The logo for a setting's value ("" is the default, synthwave).
export function logoSrc(name) {
  return (LOGOS.find(([k]) => k === name) ?? LOGOS[0])[2];
}
