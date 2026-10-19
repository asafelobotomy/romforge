import { statusLabel, released } from "./labels.js";

// The Development tab: groups of (label, value, link?) with only what is
// known; an empty group isn't shown. Studios and people Wikidata names get cards (with a logo or photo);
// what only libretro knows stays a line of text.
const roleLabel = { developer: "Developer", publisher: "Publisher", director: "Director", designer: "Designer", composer: "Composer" };
export const studioRoles = ["developer", "publisher"];
export const peopleRoles = ["director", "designer", "composer"];
export function cardsFor(d, roles) {
  return roles
    .map((role) => [roleLabel[role], (d.credits ?? []).filter((c) => c.role === role)])
    .filter(([, cs]) => cs.length);
}
export function devGroups(f, d) {
  const carded = new Set((d.credits ?? []).map((c) => c.role));
  const text = (role, v) => [roleLabel[role], carded.has(role) ? null : v];
  const g = [
    ["Studios", [text("developer", f.developer), text("publisher", f.publisher), ["Country", f.country]]],
    ["Credits", [text("director", f.director), text("designer", f.designer), text("composer", f.composer)]],
    ["Release", [["Released", released(f)], ["Serial", f.serial], ["Rating", f.esrb]]],
    [
      "Identification",
      [
        ["Status", statusLabel[d.row.status]],
        ["DATs", d.dats.join(", ")],
        ["Wikidata", f.wikidata, f.wikidata ? `https://www.wikidata.org/wiki/${f.wikidata}` : null],
      ],
    ],
  ];
  const cardsOf = { Studios: studioRoles, Credits: peopleRoles };
  return g
    .map(([h, rows]) => [h, rows.filter((r) => r[1])])
    .filter(([h, rows]) => rows.length || cardsFor(d, cardsOf[h] ?? []).length);
}
