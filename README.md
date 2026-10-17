# romforge

Catalog, identify, organize, enrich and launch retro ROM collections. Rust
core, SQLite, a CLI, and a Tauri 2 + Svelte desktop app.

Scanning and identifying only read your files. Only `organize apply` moves
and renames them, and only `cleanup apply` repacks them, each after a dry
run and journaled for undo. See [PLAN.md](PLAN.md) for the milestones and what is
still open.

## Quick start

This runs against the synthetic fixtures in the repo (no real ROMs) and
identifies them end to end:

```bash
export ROMFORGE_DB=/tmp/romforge-demo.db
cargo run -p romforge-cli -- dat import fixtures/dats/*.dat
cargo run -p romforge-cli -- scan fixtures/
cargo run -p romforge-cli -- report
cargo run -p romforge-cli -- report --dat 7 --missing
```

`scan` prints one line per file or archive member:

```
matched       roms/nes/Test NES (iNES).nes [ines]  Test NES (iNES)
matched       roms/archives/Mixed.7z#Test MD.md  Test MD
candidate     roms/gb/CRC Only.gb  CRC Only
duplicate     roms/gb/Test GB.gb  Test GB
unknown       roms/unknown.bin
scanned 17 entries (19.8 KiB) in …/fixtures: 14 matched, 1 candidate, 1 unknown, 0 wrong_system, 1 duplicate; skipped 12 non-ROM files; skipped 1 symlinked folder
```

Only files whose extension some system uses (the `extensions` column in
`crates/romforge-core/src/systems.rs`) are hashed, plus `.zip`/`.7z`; the
same rule applies to archive members, so a `.url` or `.nfo` in a zip is
skipped. `--all-files` hashes everything. The folder you name may be a
symlink, but symlinked folders inside it are never followed. Files and
archives are hashed on one thread per CPU (at most 8), largest first; a
single solid `.7z` is still read as one stream. `-j 1` hashes one file at a
time, kinder to a spinning disk. Long scans show progress on stderr; `-q`
keeps it quiet.

On your own collection, get DATs first, then scan a folder.

```bash
romforge-cli dat download SNES PlayStation    # or --all for every system
```

Cartridge systems (`[no-intro]` in `dat systems`) get No-Intro DATs from
[libretro-database](https://github.com/libretro/libretro-database)'s mirror,
because DAT-o-MATIC only hands DATs out through its download form. Disc
systems (`[redump]`) get Redump's game and BIOS DATs from redump.org. Home
computers (`[tosec]`) get TOSEC's DATs from the latest pack on
[tosecdev.org](https://www.tosecdev.org) (one ~100 MB download, skipped once
that release is imported). `--extras` adds libretro's ROM hack and homebrew
DATs (`[hacks]`, `[homebrew]`). Run it again to update; an unchanged DAT is
left alone. Downloads are spaced
out per site and never saved to disk; the DAT ends up in the database.

DATs you downloaded yourself import too, as `.dat` or as a zip of one or
many (read in memory): `romforge-cli dat import ~/Downloads/No-Intro*.zip`.
`romforge-cli dat check` compares imported No-Intro DATs with DAT-o-MATIC's
latest versions.

Use `--system NES` (keys: `romforge-cli dat systems`) or `--dat ID` to say
what a folder should contain; without it, every DAT is tried. Without
`ROMFORGE_DB` the database lives at `~/.local/share/romforge/romforge.db`.

Rescans are incremental: a file whose size and modification time haven't
changed since it was hashed keeps its hashes (`--rehash` reads everything
again). `romforge-cli games` lists games the way the app does.

## How identification works

- Every file, and every member of a `.zip` or `.7z`, is hashed as a stream
  (CRC32, MD5, SHA1). Archives are never unpacked to disk.
- A `.chd` is read as its tracks: each CD track is rebuilt as the Redump
  `.bin` it came from and hashed as a member; a DVD CHD as one `.iso`.
- If a copier header is found, the data is hashed a second time without it.
  The one header table is in `crates/romforge-core/src/headers.rs`:
  iNES/NES 2.0 (16 bytes, `NES\x1A`), FDS (16, `FDS\x1A`), Lynx (64, only
  with `LYNX` magic), Atari 7800 (128, `ATARI7800` at offset 1), SNES copier
  (512, when `size % 1024 == 512`).
- **matched**: size + SHA1, or size + MD5 when the DAT has no SHA1.
- **candidate**: only size + CRC32 agree, because the DAT has nothing
  stronger. A stronger hash that disagrees rules the ROM out.
- **wrong system**: no hit in the folder's scope, but a real hit in another
  system's DAT.
- **duplicate**: matched ROMs that an earlier file (in path order) already
  provides.
- **unknown**: nothing.

A game is **have** when all its data ROMs are matched. Cue sheets are text
that tools rewrite freely, so a disc whose tracks all match but whose `.cue`
differs from the DAT's is still have, marked `cue differs` in
`report --dat`.

One hash may belong to many games; every hit is kept. A DAT's source path,
SHA-256, import time and `<clrmamepro header=…>` hint are recorded.
Importing a new version of a DAT (same name, same source) replaces the old
one. Importing many DATs at once (a zip, `dat download`) re-identifies the
library once at the end, not after every DAT.

## Organize (dry run, apply, undo)

```bash
romforge-cli organize plan --from ~/roms --dest ~/roms-sorted --1g1r -v   # changes nothing
romforge-cli organize apply --from ~/roms --dest ~/roms-sorted --1g1r
romforge-cli organize undo                                  # latest batch
romforge-cli organize history
```

Only files under `--from` move (it is required, so a plan never reaches
other folders you scanned). Matched files go to `<dest>/<system>/<DAT name>` (archives keep their
`.zip`/`.7z`, loose files their extension; multi-file discs get a folder).
Nothing is overwritten, and a file that changed since it was moved is left
where it is by undo. Archives are moved whole, never split. A loose disc
moves only as a complete set, or with its own cue sheet listing exactly its
tracks (then every name is kept so the sheet still works). `--1g1r` keeps
one version per title: a release over a beta/proto, then `--regions` order
(default `USA,World,Europe,Japan`), then the highest revision. `--system
KEY` narrows it down. `-v` shows why each file stays. A move across drives
is a copy checked byte for byte, keeping the file's times, then a delete.

## Metadata and art

```bash
romforge-cli enrich --art boxart,snap
romforge-cli info "Super Mario World"
```

From [libretro-database](https://github.com/libretro/libretro-database)
(genre, developer, publisher, franchise, release date, players) and
[thumbnails.libretro.com](https://thumbnails.libretro.com) (box art, title
screens, snaps), for games you have complete. Art is cached under
`$ROMFORGE_CACHE` (default `~/.cache/romforge`); a second run asks for
nothing new. `--refresh` fetches again.

## Launch

```bash
romforge-cli emulator list --all
romforge-cli launch "Super Mario World" --dry-run
romforge-cli launch "Silent Hill" -e duckstation
romforge-cli emulator add bsnes --system SNES --program bsnes --args "--fullscreen {rom}"
```

RetroArch (on `PATH` or the Flatpak) with the system's first installed core,
else a known standalone emulator (DuckStation, PCSX2, Dolphin, PPSSPP,
Flycast, mGBA, melonDS, xemu, RPCS3, Mednafen, Cemu; `PATH` or Flatpak).
Emulators you add come first. A ROM in a zip/7z goes to RetroArch as the
archive; standalone emulators only get archives if they read them
(`--archives`), since romforge never extracts. A disc starts from its sheet.

## Cleanup and rename (dry run, apply, undo)

```bash
romforge-cli cleanup plan -v      # changes nothing
romforge-cli cleanup apply
romforge-cli cleanup undo         # latest batch, or --batch N
romforge-cli cleanup history
```

Every game in the Source folder that is verified by hash (or identified by
you) becomes one file named by its DAT, in `<Library>/<system>/` (the
Source when no Library is set): a zip for cartridges, holding the ROMs and
a `romforge/` folder with the art and `<name>.romforge.md`; a CHD for discs
(`.bin` tracks or one `.iso`), with the art and details in a folder of the
same name next to it. ROM data is streamed from the old file or zip; only
7z members and big zip members are unpacked, into the staging folder. Each
new file is read back and every ROM re-hashed before anything else
happens, and nothing is ever overwritten. Without Keep originals, the old
files (and other verified copies in Source) then move to `.romforge-trash/`
next to them, but only when everything in them was repacked; an archive
holding anything else stays. `undo` deletes the new files and puts the
originals back. Refresh never scans `.romforge-trash`.

## Desktop app

Needs Node and the [Tauri Linux dependencies](https://tauri.app/start/prerequisites/)
(`webkit2gtk-4.1`). It uses the same database as the CLI. The layout follows
tinyMediaManager: a toolbar (Refresh, Add folder, Search, Enrich, Cleanup
and rename, Play), platforms down the left, one row per game with columns
for what has been gathered (identified, metadata, box art, title screen,
snap), and a detail panel with Details, Development, Series and Links tabs.
DATs and Organize are at the foot of the platform list.

**Search** (toolbar, or `romforge-cli search queue|find|name|forget|wikidata`)
goes through what needs you, one game at a time: files no hash identified
(pick a DAT title it resembles, or give your own title and platform, e.g.
for a translation) and games Wikidata couldn't match on its own. Like
tinyMediaManager's search window: pick a scraper (DAT titles offline, or
Wikidata), search, compare results by year, platform and fit with box art
and the DAT's ROMs against your file, and choose what to fetch for the game
once you take one. Titles you
pick show as "identified by you" and are never counted as verified.

**Enrich** also asks Wikidata (free, no account) for developer, publisher,
country, director, composer, the series in order, and IDs that make the
Links tab point at each game's own MobyGames, RetroAchievements, Wikipedia,
IGDB and HowLongToBeat page.

With keys in Settings, Enrich also fills art libretro lacks (hero, logo,
icon, 3D box, and any missing box art or screenshots) from ScreenScraper,
then SteamGridDB, then IGDB, each only asked for what's still missing.

**Settings** (toolbar, or `romforge-cli settings`): a **Source** folder of
unprocessed ROMs, an optional **Library** folder that Cleanup and rename
sorts processed games into, **Keep originals** (Source files stay
untouched; cleanup only writes new files into the Library), and the
**staging** folder (default `~/.cache/romforge/staging`). Refresh scans the Source and Library.
Source and Library can't be inside one another, and staging stays outside
both.

```bash
romforge-cli settings --source ~/roms/incoming --library ~/roms/library --keep-originals true
romforge-cli refresh
```

```bash
cd app && npm install && npm run tauri dev
```

`npm run tauri build -- --no-bundle` builds `target/release/romforge-app`.

## Layout

- `crates/romforge-core` — all logic (library): `scan`, `identify`, `dat`,
  `fetch`, `organize`, `cleanup`, `enrich`, `disc`, `launch`; one table of systems
  (`systems.rs`) and one of copier headers (`headers.rs`)
- `crates/romforge-cli` — thin CLI
- `app/` — Tauri 2 + Svelte shell
- `fixtures/` — tiny generated test files and DATs. Regenerate with
  `cargo test -p romforge-core --test fixtures -- --ignored regenerate`.

## License

MIT (see [LICENSE](LICENSE)). The systems table is adapted from
[remustwo](https://github.com/asafelobotomy/remustwo) (MIT).
