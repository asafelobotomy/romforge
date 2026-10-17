# romforge plan

## Goal

One desktop app to catalog, identify, organize and enrich a retro ROM
collection. Rust core, Tauri 2 + Svelte UI, SQLite. Free licenses only
(MIT / Apache / BSD style).

## Rule

**No milestone starts before the previous one is solid** — meaning it works
end to end on the real collection, not just on fixtures.

On 2026-10-17 the owner chose to build milestones 2–5 before milestone 1
was proven on the whole collection (it was run on one 30-file library). All
five now exist in the core, CLI and app; what is left to prove on real data
is listed under each milestone as **Open**.

## Milestones

1. **Scan and identify** — import Logiqx/No-Intro DATs, scan
   folders and zip/7z members, hash raw and header-stripped data, match on
   size + SHA1/MD5, have/missing per DAT. Read-only: never moves or renames a
   file. Also CHD tracks (milestone 4). `dat download` fetches DATs (terms checked 2026-10-17); where from
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
   **Open:** scan the rest of the collection; run CI (no remote yet).
2. **Organize** — `organize plan|apply|undo|history`: rename/move matched
   files into `<dest>/<system>/…` named by the DAT, a dry run first, every
   move journaled in the database for undo, nothing ever overwritten
   (hard link + delete, rename, or a byte-checked copy across drives).
   Archives move whole (never split or repacked). Loose disc sets move only
   complete, or with their own sheet listing exactly their tracks, names
   kept. 1G1R keeps one version per title: releases over betas/protos, then
   region order (`--regions`, default USA, World, Europe, Japan), then
   highest revision. Tried on a copy of the real library: 22 moves, rescan,
   undo restored the tree exactly.
   **Open:** apply on the real collection (owner's call).
3. **Enrich** — `enrich`, `info`: metadata (genre, developer, publisher,
   franchise, release date, players, some descriptions) from
   libretro-database's `metadat/<field>/` files and box art / title / snap
   images from thumbnails.libretro.com, for the games you have complete,
   matched by exact DAT name or ROM CRC32. Cached in the database and under
   `$ROMFORGE_CACHE` (default `~/.cache/romforge`); nothing is written next
   to ROMs. libretro was used instead of the three named sources because it
   needs no account and is keyed by the same No-Intro/Redump names.
   **Open:** ScreenScraper and IGDB (need the owner's API keys); LaunchBox
   DB (a large name-keyed XML).
4. **Discs** — Redump DATs (milestone 1), cue and gdi sheets (scan reports
   tracks a sheet names that are missing; organize uses the sheet), and CHD:
   each CD track is rebuilt as its Redump `.bin` (subcode dropped, track
   padding skipped, audio byte order restored, unstored pregap as silence)
   and hashed as a member; a DVD CHD hashes as its `.iso`. A disc whose
   tracks all match but whose cue differs is "have (cue differs)".
   **Open:** no real CHD has been scanned yet (tests write CHDs with the
   `chd` crate's encoder); GD-ROM CHDs follow the format but are untested.
5. **Launch** — `launch`, `emulator list|add|remove`: RetroArch (on `PATH`
   or Flatpak) with the system's first installed core from the `cores`
   column, else a standalone emulator from the `emulators` column
   (DuckStation, PCSX2, Dolphin, PPSSPP, Flycast, mGBA, melonDS, xemu, RPCS3,
   Mednafen, Cemu; on `PATH` or Flatpak), with user-added emulators first.
   Archives go to RetroArch as is; standalone emulators get them only if
   they read archives, since romforge never extracts. Discs start from
   their sheet. `--dry-run` prints the command.
   **Open:** no emulator is installed on the dev machine; tested with
   stand-in scripts only.

## UI and data milestones (decided 2026-10-17)

Layout after tinyMediaManager: a toolbar on top (Refresh, Search, Enrich,
Cleanup & rename, Play), platforms down the left (icon and name), one list
of games in the middle (one row per game; unidentified files are their own
rows) with columns showing what has been gathered, and a detail panel on the
right with tabs: Details (box, banner, hero, 3D box, title, snap, icon,
description, players, genre), Development (developer, publisher, country,
credits), Series (related games in order, with years) and Links (MobyGames,
RetroAchievements, Wikipedia, IGDB, HowLongToBeat).

6. **Layout** *(built)* — the layout above on the data romforge already has;
   Refresh rescans every source folder; Links as plain search links (opened
   with Tauri's opener plugin, https only). Search and Cleanup & rename are
   in the toolbar but disabled until milestones 8 and 10; art libretro
   lacks shows as empty slots until milestone 9.
7. **Settings and refresh** *(built)* — Settings for the Source folder, an
   optional Library folder, Keep originals, the staging folder and optional
   credentials (ScreenScraper developer + user login, SteamGridDB key, IGDB
   via a Twitch app; stored locally, shown masked). Refresh scans Source
   and Library and skips files whose path, size and modification time are
   unchanged (`--rehash` reads everything again); the game list is drawn
   virtually so tens of thousands of rows stay quick. ScreenScraper needs
   developer credentials issued to an app: the owner registers romforge, or
   uses their own.
8. **Search** *(built)* — a queue dialog going game by game through every
   row not verified by hash and every game whose Wikidata match is
   ambiguous or missing: DAT title suggestions (shared words, limited to
   platforms using the file's extension), your own title and platform, or
   a Wikidata item (from the kept candidates, or an id you paste). Titles
   picked by hand are "identified by you" (status `manual`, house rule 8),
   stored by SHA1 + size so they survive moves and rescans; they get
   metadata, art and Play like any game. CLI: `romforge-cli search`.
9. **Deeper enrich** — Wikidata *(built)*: per platform, one SPARQL query
   lists its video games; a game is matched when exactly one has the same
   title (narrowed by release year), ambiguous ones are kept for Search.
   Stores developer, publisher, country, director, composer, designer,
   genre, modes, the series with its ordinal and every member, and IDs for
   MobyGames, IGDB, HowLongToBeat, RetroAchievements and Wikipedia (direct
   links). Requests are 2 s apart; 429/503 answers are retried after
   Retry-After. Platform items are in the `wikidata` column of
   `systems.rs`, each checked against Wikidata; still missing: Lynx,
   TurboGrafx-CD, Neo Geo Pocket, 3DO, Neo Geo CD, PC-FX, CD-i, CD32,
   ColecoVision, Intellivision, Vectrex, PC-98, X68000, Atari 5200, MSX2,
   Amstrad CPC. Keyed art sources *(built, not yet tried with real keys)*:
   ScreenScraper (by ROM hashes: 2D/3D box, wheel logo, fanart, title,
   screenshot), then SteamGridDB (exact title: grid, hero, logo, icon), then
   IGDB (exact title + platform: cover, artwork, screenshot), each filling
   only what is still missing and asked once per game (`--refresh` asks
   again). Their system ids (`screenscraper`, `igdb` columns) are from
   memory and must be checked once keys are entered. A source that answers
   "rate limited" is left alone for the rest of the run.
10. **Cleanup & rename** — repack each game into one archive named by the
    owner's title preferences, with a `romforge/` folder inside holding the
    art and a `.romforge.md` / `.nfo` with its details, under the guardrails
    of house rule 7. Decided 2026-10-17: archives are named by the DAT name
    as is ("Super Mario World (USA).zip"); cartridge games are repacked as
    zip; disc games become CHD (CHD can't hold extra files) with art and
    details in a sidecar folder named like the game next to it.

## Later ideas (parked, do not build yet)

- Arcade: MAME / FBNeo DATs (libretro mirrors them); merged/split sets need
  their own matching rules.
- Online lookups for files that stay unknown: RetroAchievements hashes,
  ScreenScraper, Hasheous (accounts / keys).
- `.m3u` playlists for multi-disc games (organize would write a new file).
- Play history and favourites.

- Scan-time system hints from extension or header to shortlist DATs.
- Redump update check without re-downloading (Redump has no version index).
- Parent/clone data for 1G1R (titles are grouped by name today), and the
  regions seed from remustwo.
- RAR support needs a free pure-Rust decoder; none known yet.
