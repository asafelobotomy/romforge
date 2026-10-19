// Pictures for the facts in Details: genre icons (from game-icons.net,
// see gameIcons.js), the ESRB rating as a badge, region flags and a star
// rating.
import { GAME_ICONS } from "./gameIcons.js";

// Genre words -> icon. The first that matches a genre's name wins.
const GENRES = [
  [/role.?play|rpg/i, "rpg"],
  [/beat.?'?em|brawler|fight/i, "fight"],
  [/shoot.?'?em|shmup/i, "shmup"],
  [/shoot|gun|fps/i, "shooter"],
  [/platform/i, "platform"],
  [/racing|driving|kart/i, "racing"],
  [/sport|football|soccer|golf|tennis|basketball|baseball|hockey|wrestling|boxing/i, "sports"],
  [/puzzle|logic|tetris/i, "puzzle"],
  [/strateg|tactic|war/i, "strategy"],
  [/flight|plane/i, "flight"],
  [/simulat|life/i, "simulation"],
  [/horror|survival/i, "horror"],
  [/music|rhythm|danc/i, "music"],
  [/pinball/i, "pinball"],
  [/adventure|point.?and.?click|visual novel/i, "adventure"],
  [/stealth/i, "stealth"],
  [/board|card|casino|party|quiz|trivia/i, "board"],
  [/education|learn/i, "education"],
  [/action/i, "action"],
];

/// The genres in a metadata value ("Action, Adventure", "Role-playing
/// (RPG)"), each with its icon's SVG.
export function genres(value) {
  if (!value) return [];
  const seen = new Set();
  return value
    .split(/\s*[,;/]\s*/)
    .map((g) => g.trim())
    .filter((g) => g && !seen.has(g.toLowerCase()) && seen.add(g.toLowerCase()))
    .map((name) => {
      const icon = GAME_ICONS[GENRES.find(([re]) => re.test(name))?.[1] ?? "other"];
      return {
        name,
        svg: `<svg viewBox="0 0 512 512" width="22" height="22" fill="currentColor" aria-hidden="true">${icon.svg}</svg>`,
      };
    });
}

/// A game-icons.net icon by its key in gameIcons.js (the sidebar's
/// "library" and "unknown"), as an SVG in the text colour.
export function iconSvg(key, size = 26) {
  const icon = GAME_ICONS[key];
  return icon
    ? `<svg viewBox="0 0 512 512" width="${size}" height="${size}" fill="currentColor" aria-hidden="true">${icon.svg}</svg>`
    : "";
}

/// The icons in use and who made them, for Credits.
export function iconCredits() {
  const by = {};
  for (const i of Object.values(GAME_ICONS)) (by[i.author] ??= []).push(i.name);
  return Object.entries(by).map(([author, names]) => ({ author, names: names.sort() }));
}

// ESRB ratings as their letters and words.
const ESRB = [
  [/^\s*(EC)\b/i, "EC", "Early Childhood"],
  [/^\s*(E10\+?)/i, "E10+", "Everyone 10+"],
  [/^\s*(K-?A)\b/i, "K-A", "Kids to Adults"],
  [/^\s*(E)\b/i, "E", "Everyone"],
  [/^\s*(T)\b/i, "T", "Teen"],
  [/^\s*(M)\b/i, "M", "Mature 17+"],
  [/^\s*(AO)\b/i, "AO", "Adults Only 18+"],
  [/^\s*(RP)\b/i, "RP", "Rating Pending"],
];

/// "E - Everyone" -> { code: "E", label: "Everyone" }, or null.
export function esrb(value) {
  if (!value) return null;
  const hit = ESRB.find(([re]) => re.test(value));
  return hit ? { code: hit[1], label: hit[2] } : null;
}

// Region names as written in DAT names -> two-letter codes, for those the
// parsed codes don't line up with.
const REGION_CODE = {
  usa: "US", japan: "JP", europe: "EU", uk: "GB", germany: "DE", france: "FR", spain: "ES",
  italy: "IT", korea: "KR", china: "CN", taiwan: "TW", "hong kong": "HK", brazil: "BR",
  australia: "AU", canada: "CA", netherlands: "NL", sweden: "SE", russia: "RU", asia: null,
  scandinavia: null, world: null,
};

function flagOf(code) {
  if (!/^[A-Z]{2}$/.test(code)) return null;
  return String.fromCodePoint(...[...code].map((c) => 0x1f1e6 + c.charCodeAt(0) - 65));
}

/// A name's regions as flags: { flag, name } each ("World" as a globe;
/// regions without a flag keep their name).
export function regionFlags(n) {
  if (!n?.regions?.length) return [];
  return n.regions.map((name) => {
    const key = name.toLowerCase();
    if (key === "world") return { flag: "🌐", name };
    const code = REGION_CODE[key] ?? (name.length === 2 ? name.toUpperCase() : null);
    return { flag: code ? flagOf(code) : null, name };
  });
}

/// "4.7 / 5 (1363 votes)" -> { value, max, votes, pct }, or null.
export function stars(value) {
  const m = /([\d.]+)\s*\/\s*([\d.]+)(?:\s*\(([\d,]+)\s*votes?\))?/i.exec(value ?? "");
  if (!m) return null;
  const v = parseFloat(m[1]);
  const max = parseFloat(m[2]) || 5;
  return { value: v, max, votes: m[3] ? Number(m[3].replace(/,/g, "")) : null, pct: Math.max(0, Math.min(100, (v / max) * 100)) };
}
