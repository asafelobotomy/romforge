// OCR for a manual's scanned pages (milestone 26): Tesseract's WebAssembly
// build (tesseract.js 7, Apache-2.0) turns a page's pixels into words; the
// words are kept in romforge's database per page and language, so a page
// is only ever recognised once. Everything is served by romforge itself:
// the worker and engine from the app, the language files from the cache
// folder (downloaded when asked for) — never a CDN.
import { invoke } from "@tauri-apps/api/core";
import Tesseract from "tesseract.js/dist/tesseract.esm.min.js";
import workerUrl from "tesseract.js/dist/worker.min.js?url";
import simdCore from "tesseract.js-core/tesseract-core-simd-lstm.wasm.js?url";
import plainCore from "tesseract.js-core/tesseract-core-lstm.wasm.js?url";
import { artSrc } from "./lib.js";

// The smallest module using a SIMD instruction (as wasm-feature-detect).
const SIMD = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11]);
const abs = (u) => new URL(u, location.href).href;
// Words Tesseract is less sure of than this are left out (noise on art).
const MIN_CONF = 55;

const workers = new Map();

/// The OCR languages ({code, name, downloaded}) and their folder.
export async function languages() {
  const [langs, dir] = await invoke("ocr_languages");
  return { langs, dir };
}

async function worker(lang) {
  if (!workers.has(lang)) {
    const make = async () => {
      const { langs, dir } = await languages();
      if (!langs.find((l) => l.code === lang)?.downloaded) await invoke("ocr_download", { code: lang });
      const core = WebAssembly.validate(SIMD) ? simdCore : plainCore;
      return Tesseract.createWorker(lang, 1, { workerPath: abs(workerUrl), corePath: abs(core), langPath: artSrc(dir), gzip: false, cacheMethod: "none" });
    };
    const w = make();
    workers.set(lang, w);
    w.catch(() => workers.delete(lang));
  }
  return workers.get(lang);
}

/// Recognise pdf.js page `p` (drawn unrotated, about 2000 px wide): its
/// words with boxes as fractions of the page.
async function recognize(p, lang) {
  const base = p.getViewport({ scale: 1, rotation: 0 });
  const view = p.getViewport({ scale: Math.min(3, 2000 / base.width), rotation: 0 });
  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(view.width);
  canvas.height = Math.floor(view.height);
  await p.render({ canvas, viewport: view }).promise;
  const { data } = await (await worker(lang)).recognize(canvas, {}, { blocks: true });
  const [W, H] = [canvas.width, canvas.height];
  const words = [];
  for (const b of data.blocks ?? [])
    for (const para of b.paragraphs)
      for (const line of para.lines) {
        const kept = line.words.filter((w) => w.confidence >= MIN_CONF && w.text.trim());
        kept.forEach((w, i) => {
          const { x0, y0, x1, y1 } = w.bbox;
          words.push({ text: w.text.trim(), rect: [x0 / W, y0 / H, (x1 - x0) / W, (y1 - y0) / H], conf: w.confidence, eol: i === kept.length - 1 });
        });
      }
  return words;
}

/// Page `n`'s recognised words: kept ones, or (with `run`) recognised now
/// and kept. Null when it hasn't been recognised and `run` is false.
export async function pageWords(doc, n, rowId, lang, run) {
  const kept = await invoke("manual_ocr", { rowId, page: n, lang });
  if (kept || !run) return kept;
  const words = await recognize(await doc.getPage(n), lang);
  await invoke("save_manual_ocr", { rowId, page: n, lang, words });
  return words;
}

/// The words as pdf.js text content, for its TextLayer: each word a text
/// item placed on the unrotated page (pdf.js applies the page's rotation).
export function asTextContent(p, words) {
  const [x0, y0, x1, y1] = p.view;
  const [W, H] = [x1 - x0, y1 - y0];
  const items = words.map((w) => {
    const [x, y, ww, h] = w.rect;
    const fh = h * H;
    return { str: w.eol ? w.text : w.text + " ", dir: "ltr", width: ww * W, height: fh, transform: [fh, 0, 0, fh, x0 + x * W, y0 + (1 - (y + h)) * H], fontName: "ocr", hasEOL: w.eol };
  });
  return { items, styles: { ocr: { fontFamily: "sans-serif", ascent: 0.8, descent: -0.2, vertical: false } }, lang: null };
}

/// Every recognised page's text, by page (for search).
export async function ocrTexts(rowId, lang) {
  return new Map(((await invoke("manual_ocr_texts", { rowId, lang }).catch(() => [])) ?? []).map(([n, t]) => [n, t]));
}
