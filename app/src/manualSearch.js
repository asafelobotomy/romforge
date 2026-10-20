// Searching a manual's text (milestone 26): each page's text from pdf.js,
// kept per document once read, or for a scanned page its OCR text (`ocr`,
// page → text, from what has been recognised). `find` says how many pages
// are scans not recognised yet.

const texts = new WeakMap();

/// Page `n`'s text, words joined by single spaces.
export async function pageText(doc, n) {
  let pages = texts.get(doc);
  if (!pages) texts.set(doc, (pages = new Map()));
  if (!pages.has(n)) {
    const content = await (await doc.getPage(n)).getTextContent();
    pages.set(n, content.items.map((i) => i.str ?? "").join(" ").replace(/\s+/g, " ").trim());
  }
  return pages.get(n);
}

/// Every page where `query` appears (any case), with a snippet around its
/// first match; `hasText` is false for a manual with no text at all, and
/// `unread` lists the scanned pages with no OCR text yet.
export async function find(doc, query, ocr = new Map()) {
  const q = query.trim().toLowerCase();
  const hits = [];
  const unread = [];
  let hasText = false;
  for (let n = 1; n <= doc.numPages; n++) {
    let t = await pageText(doc, n);
    if (!t) {
      if (ocr.has(n)) t = ocr.get(n);
      else unread.push(n);
    }
    if (t) hasText = true;
    const at = q ? t.toLowerCase().indexOf(q) : -1;
    if (at >= 0) {
      const from = Math.max(0, at - 40);
      hits.push({ page: n, before: (from > 0 ? "…" : "") + t.slice(from, at), match: t.slice(at, at + q.length), after: t.slice(at + q.length, at + q.length + 50) + "…" });
    }
  }
  return { hits, hasText, unread };
}
