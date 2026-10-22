# House rules

1. **Library first.** All logic lives in `crates/romforge-core`. The CLI and
   the Tauri app are thin shells that call it. If a shell has an `if` about
   ROMs, it belongs in the core.
2. **CI green from the first commit.** `cargo fmt --check`, `cargo clippy
   -- -D warnings` and `cargo test` must pass before every commit.
3. **No ROMs or DATs in git.** Only the tiny hand-made fixtures in
   `fixtures/`. Real DATs and dumps stay on the user's disk.
4. **One table of systems** (`crates/romforge-core/src/systems/table.rs`) and **one
   table of copier headers** (`headers.rs`). Never a second detector or a
   second list.
5. **No pile of docs, reports or scripts.** README (for users: short),
   GUIDE.md (the technical detail), PLAN.md and this file.
   Ideas for later milestones go in PLAN.md, not into code.
6. **Stay in the milestone.** See PLAN.md. Scanning and identifying never
   move, rename or write ROM files; only organize and cleanup do, and only
   after a dry run, journaled for undo.
7. **No external tools.** No shelling out to 7z, unzip, chdman, etc. Archives
   are read as streams. Only cleanup & rename (milestone 10) rewrites ROMs,
   with these guardrails: repack stream to stream (no temporary extraction
   where avoidable), re-hash the new archive against the DAT before the
   original goes, keep originals in a trash folder for undo, and never let
   extra files (art, `.nfo`, `.romforge.md`) shadow the ROM: they go in a
   `romforge/` folder inside the archive, the ROM stays at the top level.
   Besides the emulators (and asking them for their own lists or
   installers: ScummVM, MAME, RPCS3), the one program romforge runs is
   `flatpak`, to start Flatpak emulators and to install emulators from
   Flathub when the user asks (`obtain/flathub.rs`).
8. **Matching is strict.** A match needs size + SHA1 or size + MD5. CRC32
   alone is only a candidate. Never fall back to a default system, never
   `LIMIT 1` over a hash that may belong to many games. A title the user
   picks by hand is its own status ("identified by you") and is never shown
   or counted as verified.
9. **Licenses:** dependencies must be MIT, Apache-2.0, BSD, Zlib, ISC,
   Unlicense or similar. MPL-2.0 is accepted only for the crates Tauri
   itself pulls into the app, each named in `deny.toml`; never in
   romforge-core or the CLI.
10. **Data sources in order of preference.** (1) Free, no account and no
    key: libretro, Wikidata, plain search links. (2) Free key, no account:
    ScreenScraper with app credentials. (3) Free account and free key:
    SteamGridDB, IGDB. A later tier only fills what earlier tiers left
    empty, and keys are optional settings, never required. Free data
    published without a licence (OpenVGDB, LaunchBox's Metadata.zip) may be
    used, but never shipped: no copies in git, fixtures or releases; the user
    downloads it from inside romforge, and it fills only what the open
    sources leave empty.
11. **No code file over 400 lines.** Rust, Svelte, JS and CSS, tests
    included. A long module becomes a folder of child modules behind the
    same root file; a long component becomes smaller components. The one
    exemption is the systems table (`systems/table.rs`), which is data.
    `crates/romforge-core/tests/file_length.rs` enforces it.
