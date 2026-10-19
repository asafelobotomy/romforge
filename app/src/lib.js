// Helpers more than one screen needs.

export function size(n) {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MiB` : n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`;
}

// A picture file as the page loads it: through the app's `art://` scheme
// (only romforge's art folders are served), so a grid of covers loads just
// what is on screen, straight from disk.
const ART_BASE = navigator.userAgent.includes("Windows") ? "http://art.localhost" : "art://localhost";
export function artSrc(path) {
  if (!path) return null;
  return ART_BASE + path.split(/[\\/]/).map(encodeURIComponent).join("/").replace(/^(?!\/)/, "/");
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
