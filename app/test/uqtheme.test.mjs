// uQ's colours in the fullscreen mode (node --test app/test), and the
// vendored uq-tokens left as uQ ships it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { FS_FROM_UQ, fsVars } from "../src/uqtheme.js";
import { VERSION, applyTokens } from "../src/vendor/uq-tokens/uq-tokens.js";

const schemes = {
  light: { "--fs-bg": "#dcdce3", "--fs-text": "#23232b" },
  dark: { "--fs-bg": "#222228", "--fs-text": "#e4e4ea" },
};
const applied = (over) => ({ scheme: null, schemes, vars: { "--fs-radius": "1vh" }, desktop: true, ...over });

test("without uQ the system's light or dark applies", () => {
  assert.deepEqual(fsVars(applied(), false, null), {
    scheme: "light",
    vars: { ...schemes.light, "--fs-radius": "1vh" },
  });
  assert.equal(fsVars(applied(), true, null).scheme, "dark");
});

test("on uQ the default theme takes uQ's variant and colours, options still last", () => {
  const { scheme, vars } = fsVars(applied(), false, "dark");
  assert.equal(scheme, "dark", "uQ's variant, not the system's");
  assert.equal(vars["--fs-bg"], "var(--uq-bg)");
  assert.equal(vars["--fs-text"], "var(--uq-text)");
  assert.equal(vars["--fs-radius"], "1vh");
  assert.equal(fsVars(applied(), true, "light").scheme, "light");
  for (const v of Object.values(FS_FROM_UQ)) assert.match(v, /^var\(--uq-[a-z-]+\)$/);
});

test("a theme or colours picked by hand keep their own", () => {
  const picked = fsVars(applied({ desktop: false }), false, "dark");
  assert.equal(picked.scheme, "light");
  assert.equal(picked.vars["--fs-bg"], "#dcdce3");
  const fixed = fsVars(applied({ desktop: false, scheme: "dark" }), false, "light");
  assert.equal(fixed.scheme, "dark");
  assert.equal(fixed.vars["--fs-bg"], "#222228");
  assert.equal(fsVars(applied(), false, "sepia").vars["--fs-bg"], "#dcdce3", "not a variant");
});

test("the vendored module is uQ's 1.0.0 and checks what it sets", () => {
  assert.equal(VERSION, "1.0.0");
  const head = readFileSync(new URL("../src/vendor/uq-tokens/uq-tokens.js", import.meta.url), "utf8");
  assert.match(head.split("\n")[0], /^\/\/ uq-tokens 1\.0\.0$/);
  const set = new Map();
  const attrs = new Map();
  const root = {
    style: { setProperty: (k, v) => set.set(k, v), removeProperty: (k) => set.delete(k) },
    setAttribute: (k, v) => attrs.set(k, v),
    removeAttribute: (k) => attrs.delete(k),
  };
  applyTokens({ variant: "dark", colors: { bg: "#101010", text: "red;}body{x:y" } }, root);
  assert.equal(set.get("--uq-bg"), "#101010");
  assert.equal(set.has("--uq-text"), false, "a bad value is left out");
  assert.equal(attrs.get("data-uq-variant"), "dark");
});
