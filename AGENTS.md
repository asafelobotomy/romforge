# House rules

1. **Library first.** All logic lives in `crates/romforge-core`. The CLI and
   the Tauri app are thin shells that call it. If a shell has an `if` about
   ROMs, it belongs in the core.
2. **CI green from the first commit.** `cargo fmt --check`, `cargo clippy
   -- -D warnings` and `cargo test` must pass before every commit.
3. **No ROMs or DATs in git.** Only the tiny hand-made fixtures in
   `fixtures/`. Real DATs and dumps stay on the user's disk.
4. **One table of systems** (`crates/romforge-core/src/systems.rs`) and **one
   table of copier headers** (`headers.rs`). Never a second detector or a
   second list.
5. **No pile of docs, reports or scripts.** README, PLAN.md and this file.
   Ideas for later milestones go in PLAN.md, not into code.
6. **Stay in the milestone.** See PLAN.md. Milestone 1 never moves, renames
   or writes ROM files.
7. **No external tools.** No shelling out to 7z, unzip, chdman, etc. Archives
   are read as streams and never extracted to disk.
8. **Matching is strict.** A match needs size + SHA1 or size + MD5. CRC32
   alone is only a candidate. Never fall back to a default system, never
   `LIMIT 1` over a hash that may belong to many games.
9. **Licenses:** dependencies must be MIT, Apache-2.0, BSD, Zlib, ISC,
   Unlicense or similar.
