// Helpers more than one screen needs.

// A file size in binary units: "512 B", "3.5 KiB", "1.2 MiB", "4.7 GiB".
export function size(n) {
  if (n >= 2 ** 30) return `${(n / 2 ** 30).toFixed(1)} GiB`;
  if (n >= 2 ** 20) return `${(n / 2 ** 20).toFixed(1)} MiB`;
  if (n >= 2 ** 10) return `${(n / 2 ** 10).toFixed(1)} KiB`;
  return `${n} B`;
}

// The last part of a path, Windows' or anyone else's.
export const basename = (p) => p.split(/[\\/]/).pop();

// A picture file as the page loads it: through the app's `art://` scheme
// (only romforge's art folders are served), so a grid of covers loads just
// what is on screen, straight from disk. Parentheses stay in titles
// ("Game (USA).png"); encodeURIComponent leaves them, so encode those too
// or some fetches break on the path.
const ART_BASE = navigator.userAgent.includes("Windows") ? "http://art.localhost" : "art://localhost";
const artSeg = (s) => encodeURIComponent(s).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
export function artSrc(path) {
  if (!path) return null;
  return ART_BASE + path.split(/[\\/]/).map(artSeg).join("/").replace(/^(?!\/)/, "/");
}

// The same picture scaled down for a card `cssHeight` px tall (at the
// screen's density): the app makes and keeps a thumbnail, so a grid of
// thousands doesn't decode full-size scans and fanart. Heights go in steps
// so a resized window reuses the ones made already.
export function thumbSrc(path, cssHeight) {
  const src = artSrc(path);
  if (!src) return null;
  const h = Math.ceil((cssHeight * (window.devicePixelRatio || 1)) / 60) * 60;
  return `${src}?h=${h}`;
}
