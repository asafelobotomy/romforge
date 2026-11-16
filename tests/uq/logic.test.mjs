// Tests for uq/pages/romforge: logic.js, page.toml, the page's text and files, and its packaging.
//   node --test 'tests/uq/*.test.mjs'
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { loadLogic } from "./load-logic.mjs";
import { parseToml } from "./toml-lite.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const pageDir = join(repo, "uq", "pages", "romforge");
const L = loadLogic(join(pageDir, "logic.js"));
const manifest = parseToml(readFileSync(join(pageDir, "page.toml"), "utf8"));
const fx = (name) => JSON.parse(readFileSync(join(here, "fixtures", `${name}.json`), "utf8"));
const ok = (name) => ({ code: 0, output: fx(name) });
const files = (ext) => readdirSync(pageDir).filter((f) => f.endsWith(ext));
const read = (f) => readFileSync(join(pageDir, f), "utf8");

// ---- the manifest and logic.js agree ---------------------------------------------------------

test("page.toml's browse patterns are logic.js's", () => {
  const cmd = manifest.command.find((c) => c.id === "browse");
  assert.deepEqual(cmd.args.map((a) => a.name), L.BROWSE_ORDER);
  for (const a of cmd.args) assert.equal(a.pattern, L.PATTERNS.browse[a.name], a.name);
});

test("only reads: no command confirms, previews or changes anything", () => {
  for (const c of manifest.command) {
    assert.equal(c.confirm, undefined, c.id);
    assert.ok(c.argv[0] === "/usr/bin/romforge-cli", c.id);
    assert.ok(["overview", "browse"].includes(c.argv[1]), `${c.id} runs ${c.argv[1]}`);
  }
});

test("every [[setting]] row has a SettingRow in the page", () => {
  const qml = files(".qml").map(read).join("\n");
  for (const s of manifest.setting) assert.ok(qml.includes(`settingId: "${s.id}"`), s.id);
});

test("no command confirms, so tst_page.qml has nothing to list", () => {
  assert.equal(manifest.command.filter((c) => c.confirm).length, 0);
});

// ---- the page's text and files ---------------------------------------------------------------

const BANNED = /D-?Bus|polkit|JSON|sqlite|romforge-cli|argv|\bDAT\b|\bDATs\b|No-Intro|Redump/i;

function literals(line) {
  return [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);
}

test("the page's strings are plain", () => {
  const found = [];
  for (const f of files(".qml")) {
    for (const line of read(f).split("\n")) {
      if (/\btext:|placeholderText:|title:|detail:|help:|label:/.test(line)) found.push(...literals(line));
    }
  }
  for (const line of read("logic.js").split("\n")) {
    if (/^\s*\/\//.test(line) || /PATTERNS|pattern|RegExp|\.test\(|\.exec\(|"\^/.test(line)) continue;
    found.push(...literals(line).filter((s) => /^[A-Z].* /.test(s) || /…/.test(s)));
  }
  found.push(manifest.name);
  for (const s of manifest.setting) found.push(s.label);
  assert.ok(found.length > 30, `only ${found.length} strings found`);
  for (const s of found) assert.ok(!BANNED.test(s), `jargon in: ${s}`);
});

test("no file of the page is over 400 lines, and the QML only shows text through UqLabel", () => {
  for (const f of readdirSync(pageDir)) assert.ok(read(f).split("\n").length <= 400, `${f} is too long`);
  for (const f of files(".qml")) {
    assert.ok(!/(^|[^A-Za-z.])(Text|Label|TextEdit)\s*\{/.test(read(f)), `${f} shows text without UqLabel`);
    assert.ok(!/import\s+org\.kde/.test(read(f)), `${f} imports Kirigami`);
  }
});

test("the page never calls D-Bus or streams", () => {
  assert.ok(!/ctx\.dbus|\.stream\(/.test(files(".qml").map(read).join("\n")));
});

// ---- overview --------------------------------------------------------------------------------

test("overviewView: totals, platforms, recent games, BIOS and lists", () => {
  const v = L.overviewView(ok("overview"));
  assert.equal(v.loaded, true);
  assert.equal(v.hasGames, true);
  assert.match(v.summary, /^6 games · /);
  assert.match(v.detail, /6 verified/);
  assert.equal(v.platforms.length, 3);
  const biggest = v.platforms.reduce((a, b) => (b.games > a.games ? b : a));
  assert.equal(biggest.fraction, 1);
  assert.ok(v.platforms.every((p) => p.fraction > 0 && p.fraction <= 1));
  assert.equal(v.platforms[0].verifiedText, "all verified");
  assert.equal(v.recent.length, fx("overview").recent.length);
  assert.match(v.recent[0].added, /^2026-11-1\d$/);
  assert.equal(v.bios.length, 2);
  assert.equal(v.bios[0].emulator, "DuckStation");
  assert.deepEqual(v.missingLists.map((m) => m.name).slice(0, 1), ["Atari 7800 ProSystem"]);
  assert.equal(v.needsAttention, 2 + 4);
  assert.equal(v.attentionText, "6 things need attention.");
  assert.deepEqual(v.folders.map((f) => f.label), ["RomForge folder", "Source folder", "Library folder"]);
  assert.equal(v.folders[0].path, "/home/user/RomForge");
});

test("overviewView: an empty library says how to start, a library of unmatched files says what's next", () => {
  const empty = L.overviewView(ok("overview-empty"));
  assert.equal(empty.loaded, true);
  assert.equal(empty.hasGames, false);
  assert.equal(empty.summary, "No games yet");
  assert.match(empty.detail, /hasn't scanned any games/);
  assert.equal(empty.needsAttention, 0);
  assert.equal(empty.attentionText, "Nothing is missing.");
  assert.deepEqual(empty.folders.map((f) => f.path), ["", "", ""]);
  const unmatched = L.overviewView(ok("overview-unidentified"));
  assert.equal(unmatched.summary, "No games identified yet");
  assert.match(unmatched.detail, /4 files found/);
});

test("overviewView: a failed or odd answer is a problem, never a crash", () => {
  for (const bad of [{ error: "timed out" }, { code: 1, output: null, error: "exit 1" }, { code: 0, output: [] }, { code: 0, output: "x" }, null, undefined, {}]) {
    const v = L.overviewView(bad);
    assert.equal(v.loaded, false);
    assert.notEqual(v.problem, "");
    assert.deepEqual(v.platforms, []);
  }
  const odd = L.overviewView({ code: 0, output: { totals: { games: "many" }, platforms: [null, 3, { system: 1 }, { system: "NES", name: "NES", games: -4 }], recent: [null, { title: 5 }], bios: [7], lists: { missing: "no" } } });
  assert.equal(odd.loaded, true);
  assert.equal(odd.platforms.length, 1);
  assert.equal(odd.platforms[0].games, 0);
  assert.deepEqual(odd.recent, []);
  assert.deepEqual(odd.bios, []);
});

test("text from romforge is cleaned and cut", () => {
  const out = fx("overview");
  out.platforms[0].name = "A\u0000B\n" + "x".repeat(500);
  out.recent[0].title = "T\u001b[31mred";
  const v = L.overviewView({ code: 0, output: out });
  assert.ok(!/[\u0000-\u001f]/.test(v.platforms[0].name));
  assert.ok(v.platforms[0].name.length <= 80);
  assert.ok(!/[\u0000-\u001f]/.test(v.recent[0].title));
});

// ---- the list --------------------------------------------------------------------------------

test("gamesView: rows with platform, year and versions; paging appends", () => {
  const first = L.gamesView(ok("browse"));
  assert.equal(first.loaded, true);
  assert.equal(first.total, 13);
  assert.equal(first.rows.length, 3);
  assert.equal(first.more, true);
  assert.equal(first.shownText, "Showing 3 of 13");
  assert.equal(first.rows[0].status, "Might be a game");
  assert.equal(first.rows[1].subtitle, "Nintendo Game Boy · 1993");
  assert.equal(first.rows[2].subtitle, "Nintendo Game Boy · 2 versions");
  assert.equal(first.rows[1].verified, true);
  const next = L.gamesView(ok("browse"), first.rows);
  assert.equal(next.rows.length, 6);
  assert.equal(L.gamesView({ code: 0, output: { games: [], total: 6 } }, first.rows.concat(first.rows)).more, false);
});

test("gamesView: no match, and a failure keeps what was shown", () => {
  const none = L.gamesView(ok("browse-empty"));
  assert.equal(none.shownText, "No games match.");
  assert.equal(none.more, false);
  const failed = L.gamesView({ error: "timed out" }, [{ title: "kept" }]);
  assert.equal(failed.problem, "timed out");
  assert.deepEqual(failed.rows, [{ title: "kept" }]);
  assert.equal(L.gamesView({ code: 0, output: { total: 2, games: [{ title: "A" }, { x: 1 }, null] } }).rows.length, 1);
});

test("browseArgs builds arguments that match page.toml's patterns", () => {
  const args = L.browseArgs("Game Boy", "verified", "year", true, 100, "  zelda ");
  assert.deepEqual(args, ["Game Boy", "verified", "year:desc", "50", "100", "zelda"]);
  assert.equal(L.argsProblem("browse", args), "");
  assert.equal(L.browseArgs("all", "all", "title", false, 0, "")[5], "*");
  // a leading dash would be refused by uQ, so it is taken off
  assert.equal(L.browseArgs("all", "all", "title", false, 0, "--help")[5], "help");
  assert.equal(L.browseArgs("all", "all", "title", false, 0, "---")[5], "*");
  assert.equal(L.browseArgs("all", "all", "title", false, -5, null)[4], "0");
  assert.equal(L.browseArgs("all", "all", "title", false, 0, "a".repeat(200))[5].length, 80);
  for (const bad of [["-x", "all", "title", "50", "0", "*"], ["all", "nope", "title", "50", "0", "*"], ["all", "all", "title:sideways", "50", "0", "*"], ["all", "all", "title", "5x", "0", "*"], ["all", "all", "title", "50", "0"]])
    assert.notEqual(L.argsProblem("browse", bad), "", bad.join(","));
  assert.notEqual(L.argsProblem("scan", args), "");
});

test("every platform key romforge knows is a choice the pattern accepts", () => {
  const table = readFileSync(join(repo, "crates", "romforge-core", "src", "systems", "table.rs"), "utf8");
  const keys = [...table.matchAll(/^\s*s\(\d+,\s*"([^"]+)"/gm)].map((m) => m[1]);
  assert.ok(keys.length > 100, `found ${keys.length} keys`);
  for (const k of keys) assert.equal(L.argsProblem("browse", L.browseArgs(k, "all", "title", false, 0, "")), "", k);
});

test("choices", () => {
  const choices = L.platformChoices(L.overviewView(ok("overview")));
  assert.equal(choices[0].id, "all");
  assert.equal(choices.length, 4);
  assert.equal(L.choiceIndex(choices, "NES"), 1);
  assert.equal(L.choiceIndex(choices, "nope"), 0);
  assert.equal(L.choiceId(choices, 99), "all");
  assert.deepEqual(L.labels(L.STATUS_CHOICES), ["All games", "Verified", "Incomplete", "Not identified"]);
  assert.deepEqual(L.platformChoices(null), [{ id: "all", label: "All platforms" }]);
});

test("number formats", () => {
  assert.equal(L.count(1234567), "1,234,567");
  assert.equal(L.count(-1), "0");
  assert.equal(L.size(1234), "1.2 kB");
  assert.equal(L.size(null), "—");
  assert.equal(L.size(999999999999), "1.0 TB");
});

// ---- packaging -------------------------------------------------------------------------------

test("the packages install the whole page folder", () => {
  const pkgbuild = readFileSync(join(repo, "packaging", "aur", "PKGBUILD"), "utf8");
  assert.ok(pkgbuild.includes('install -Dm644 -t "$pkgdir/usr/share/uq/settings/pages/romforge" uq/pages/romforge/*'));
  const conf = JSON.parse(readFileSync(join(repo, "app", "src-tauri", "tauri.conf.json"), "utf8"));
  for (const kind of ["deb", "rpm"]) {
    const installed = conf.bundle.linux[kind].files;
    for (const f of readdirSync(pageDir)) {
      assert.equal(installed[`/usr/share/uq/settings/pages/romforge/${f}`], `../../uq/pages/romforge/${f}`, `${kind}: ${f}`);
    }
  }
});
