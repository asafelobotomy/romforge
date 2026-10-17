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
   file. `dat download` fetches DATs (terms checked 2026-10-17); where from
   is in the `redump` and `nointro` columns of `systems.rs`:
   - **Redump**: game zips and BIOS DATs from `redump.org/datfile/<code>/`.
   - **No-Intro**: the data license allows any reuse, but DAT-o-MATIC serves
     DATs only through its download form (throttled, may show a captcha), so
     romforge takes them from libretro-database's public mirror on GitHub
     (`metadat/no-intro`, clrmamepro format, updated about weekly). Hand
     downloads from DAT-o-MATIC still import, and `dat check` compares them
     with the public `profile.xml`.
   - **TOSEC**: home computers (and other systems neither covers), from the
     complete DAT pack of the latest release on tosecdev.org (~100 MB zip,
     read in memory; skipped when that release is already imported).
   - **Extras** (`--extras`): libretro-database's ROM hack and homebrew DATs.
   Requests to one host are kept 1 s apart (5 s for DAT-o-MATIC, per its
   robots.txt).
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
- Redump update check without re-downloading (Redump has no version index).
- Parent/clone and region data (the regions seed from remustwo) for 1G1R in
  milestone 2.
- RAR support needs a free pure-Rust decoder; none known yet.
