// How a library row's title is shown: the main row is the clean game
// name (no region/language brackets); versions under it show region,
// language, translation or hack — as flags by default, or as text.

import { regionFlags } from "./facts.js";

/// ISO 639-ish codes in DAT names → a country for a flag emoji.
const LANG_FLAG = {
  en: "US",
  eng: "US",
  ja: "JP",
  jap: "JP",
  jpn: "JP",
  fr: "FR",
  fre: "FR",
  fra: "FR",
  de: "DE",
  ger: "DE",
  deu: "DE",
  es: "ES",
  spa: "ES",
  it: "IT",
  ita: "IT",
  pt: "PT",
  por: "PT",
  nl: "NL",
  dut: "NL",
  nld: "NL",
  ru: "RU",
  rus: "RU",
  ko: "KR",
  kor: "KR",
  zh: "CN",
  chi: "CN",
  zho: "CN",
  sv: "SE",
  swe: "SE",
  no: "NO",
  nor: "NO",
  da: "DK",
  dan: "DK",
  fi: "FI",
  fin: "FI",
  pl: "PL",
  pol: "PL",
  cs: "CZ",
  cze: "CZ",
  ces: "CZ",
  hu: "HU",
  hun: "HU",
  el: "GR",
  gre: "GR",
  ell: "GR",
  tr: "TR",
  tur: "TR",
  ar: "SA",
  ara: "SA",
  he: "IL",
  heb: "IL",
  th: "TH",
  tha: "TH",
  vi: "VN",
  vie: "VN",
};

function flagOf(code) {
  if (!code || !/^[A-Z]{2}$/.test(code)) return null;
  return String.fromCodePoint(...[...code].map((c) => 0x1f1e6 + c.charCodeAt(0) - 65));
}

/// A name's languages as flags (or null when unknown), with the written code.
export function languageFlags(n) {
  if (!n?.languages?.length) return [];
  return n.languages.map((code) => {
    const key = code.toLowerCase();
    // Only languages mapped above: a language code is not a country code
    // ("ca" Catalan isn't Canada), so the rest show as written.
    const cc = LANG_FLAG[key] ?? null;
    return { flag: cc ? flagOf(cc) : null, code };
  });
}

function regionLine(n, flags) {
  if (!n?.regions?.length) return "";
  if (!flags) return n.regions.map((r) => `(${r})`).join("");
  return regionFlags(n)
    .map((r) => r.flag ?? r.name)
    .join("");
}

function langLine(n, flags) {
  if (!n?.languages?.length) return "";
  if (!flags) return n.languages.map((l) => `(${l})`).join("");
  return languageFlags(n)
    .map((l) => l.flag ?? l.code)
    .join("");
}

function bits(n) {
  if (!n) return [];
  return [n.version, n.status, n.disc && `Disc ${n.disc}`].filter(Boolean);
}

/// Label for one row in the list or picture views.
/// `flags`: region/language as emojis (Settings); else as "(USA)/(En)".
export function listLabel(r, flags = true) {
  if (r?.head) return r.label;
  const n = r?.name;
  const clean = n?.title || r?.label || r?.title || "";
  if (r?.main || !r?.category) return clean;

  switch (r.category) {
    case "region": {
      const line = [regionLine(n, flags), langLine(n, flags), ...bits(n)].filter(Boolean).join(" ");
      return line || clean;
    }
    case "edition": {
      // What sets it apart (from the core: "Director's Cut", "Greatest
      // Hits"), then its region and language like any version.
      const line = [r.edition, regionLine(n, flags), langLine(n, flags), ...bits(n), n?.unlicensed && "Unlicensed"]
        .filter(Boolean)
        .join(" ");
      return line || clean;
    }
    case "translation":
    case "patch_translation": {
      const tag = n?.translation || "Translation";
      const langs = langLine(n, flags);
      return ["🔤", langs, tag].filter(Boolean).join(" ");
    }
    case "hack":
    case "patch_hack":
      return ["🔧", n?.hack || "Hack"].filter(Boolean).join(" ");
    default:
      return r.label ?? r.title ?? clean;
  }
}
