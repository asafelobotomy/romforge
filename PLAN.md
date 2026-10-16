# romforge plan

## Goal

One desktop app to catalog, identify, organize and enrich a retro ROM
collection. Rust core, Tauri 2 + Svelte UI, SQLite. Free licenses only
(MIT / Apache / BSD style).

## Rule

**No milestone starts before the previous one is solid** — meaning it works
end to end on the real collection, not just on fixtures.

## Milestones

1. **Scan and identify** *(current)* — import Logiqx/No-Intro DATs, scan
   folders and zip/7z members, hash raw and header-stripped data, match on
   size + SHA1/MD5, have/missing per DAT. Read-only: never moves or renames a
   file.
2. **Organize** — rename/move, 1G1R, always with a dry run and undo.
3. **Enrich** — ScreenScraper, IGDB or LaunchBox DB metadata and art, cached
   locally.
4. **Discs** — Redump DATs, cue/bin/gdi, CHD.
5. **Launch** — RetroArch and standalone emulators.

## Later ideas (parked, do not build yet)

- Incremental rescans keyed on path + size + mtime, so unchanged files are
  not re-hashed.
- Parallel hashing across files.
- Scan-time system hints from extension or header to shortlist DATs.
- DAT auto-update from No-Intro / Redump (needs their terms checked).
- Parent/clone and region data (the regions seed from remustwo) for 1G1R in
  milestone 2.
- RAR support needs a free pure-Rust decoder; none known yet.
