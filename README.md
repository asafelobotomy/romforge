# romforge

Catalog, identify, organize, enrich and launch retro ROM collections. Rust
core, SQLite, a CLI, and a Tauri 2 + Svelte desktop app.

Scanning and identifying only read your files. Only `organize apply` moves
and renames them, and only `cleanup apply` and `import --apply` repack them
(into the RomForge folder), each after a dry run and journaled for undo. See [PLAN.md](PLAN.md) for the milestones and what is
still open.

## Install

Each [release](https://github.com/asafelobotomy/romforge/releases) has:

- `romforge_<version>_amd64.deb` (Debian, Ubuntu) and
  `romforge-<version>-1.x86_64.rpm` (Fedora, openSUSE): the app and
  `romforge-cli`, using the system's WebKitGTK; RetroArch and ScummVM are
  recommended;
- a `PKGBUILD` for the AUR's `romforge-bin` (Arch, Manjaro), with the
  emulators as optional dependencies;
- `romforge_<version>_amd64.AppImage`: one portable file for any distro
  (bigger: it carries its own WebKitGTK; needs FUSE);
- `romforge-cli-<version>-x86_64-linux.tar.gz`: the CLI alone;
- `SHA256SUMS`.

There is no Flatpak: romforge starts the emulators installed on your
system and works with files on any drive, which a Flatpak's sandbox
doesn't allow. A release is made by pushing a tag `vX.Y.Z` matching the
version in `Cargo.toml` and `tauri.conf.json` (`.github/workflows/release.yml`).
To build the packages yourself: `cd app && npm ci && npx tauri build`
(they land in `target/release/bundle/`).

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
scanned 17 entries (19.8 KiB) in …/fixtures: 14 matched, 1 candidate, 1 unknown, 0 wrong_system, 1 duplicate, 0 manual, 0 sheet; skipped 12 non-ROM files; skipped 1 symlinked folder
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
- If a copier header or another byte order is found, the data is hashed a
  second time in the DAT's form. The one table is in
  `crates/romforge-core/src/headers.rs`: iNES/NES 2.0 (16 bytes,
  `NES\x1A`), FDS (16, `FDS\x1A`), Lynx (64, only with `LYNX` magic),
  Atari 7800 (128, `ATARI7800` at offset 1), N64 `.v64`/`.n64` byte orders
  (hashed as `.z64`), Super Magic Drive `.smd` interleave, SNES and PC
  Engine copier headers (512, when `size % 1024 == 512`).
- **matched**: size + SHA1, or size + MD5 when the DAT has no SHA1.
- **candidate**: only size + CRC32 agree, because the DAT has nothing
  stronger. A stronger hash that disagrees rules the ROM out.
- **wrong system**: no hit in the folder's scope, but a real hit outside
  it: another system's DAT, or (for a folder scoped to one DAT) another DAT
  of the same system.
- **duplicate**: matched ROMs that another copy already provides. The copy
  that holds more of the game in one archive or folder keeps them; ties go
  by path order. Empty files match nothing.
- **manual**: no hash match in scope, but you named it in Search
  ("identified by you", or "translation / hack of X"); never verified.
- **sheet**: a cue (or GDI) sheet that matches no hash but names exactly a
  game's tracks, all matched beside it (a rewritten sheet).
- **unknown**: nothing. Search suggests what it may be (see below).

A game is **have** when all its data ROMs are matched. Cue sheets are text
that tools rewrite freely, so a disc whose tracks all match but whose `.cue`
differs from the DAT's is still have, marked `cue differs` in
`report --dat`.

When no hash matches, Search **suggests** what a file may be, each with its
evidence (name, header title, size, CRC32, the serial in its own header or
boot files, region, a BPS/UPS/IPS patch beside it, an RVZ/CSO/NKit image's
ID and size) and a band that is never counted as verified: *Patch of it
(proven)*, *Likely: CRC match*, *Likely: compressed image*, *Likely:
modified copy*, *Suggested: name match*, *Possible: similar name*. Names
are read as No-Intro, Redump, TOSEC or GoodTools (`names.rs`) and compared
with numeral vetoes ("Street Fighter II" never suggests "III";
`titles.rs`). A file whose name says translation or hack is offered as one
("translation of Dragon Quest III (Japan)"), shown with its base game's art.

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
(default `USA,World,Europe,Japan`), then the highest revision. Titles that
are one game under other names ("Aero Fighters" in the USA is "Sonic
Wings" in Japan) count as one, from Retool's clone lists (BSD-3-Clause),
which `dat download` fetches with each system's DATs. `--system
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
Flycast, mGBA, melonDS, xemu, RPCS3, Mednafen, Cemu, DOSBox Staging,
DOSBox-X, ScummVM, MAME; `PATH` or Flatpak). A Flatpak emulator is given
the game's folder for that run (`--filesystem`), since it may see only
your home folder.
Emulators you add come first. A ROM in a zip/7z goes to RetroArch as the
archive; standalone emulators only get archives if they read them
(`--archives`), since romforge never extracts. A disc starts from its sheet.

## The RomForge folder (milestone 13)

Everything romforge keeps can live in one portable folder, always named
`RomForge`, inside a folder you pick:

```
RomForge/
  romforge.db                the library
  Games/<System>/<Title>.zip cartridges (art and details inside, in romforge/)
  Games/<System>/<Title>.chd discs, with a folder of the same name beside it
  Media/<System>/<kind>/     art
  Databases/                 OpenVGDB, LaunchBox (downloaded by you)
  .romforge-staging/  .romforge-trash/
```

```bash
romforge-cli home create ~        # creates ~/RomForge; an existing library moves in
romforge-cli home show
romforge-cli import ~/roms/incoming          # scans it, shows the plan, changes nothing
romforge-cli import ~/roms/incoming --apply  # moves its identified games into Games/
romforge-cli cleanup undo                    # puts the last import back
romforge-cli home move /mnt/big              # the whole folder moves (paths follow)
romforge-cli settings --regions "USA, Europe, Japan"
```

The desktop app asks where to create it on first run. Moving or backing up
the folder takes the whole library with it; `~/.config/romforge/location`
says where it is (`ROMFORGE_HOME` overrides). Scanning still only reads:
**Scan folder** scans a folder, and only **Rename & Cleanup** (an import
into `Games/` here) moves anything. Only verified games and games you named move;
unidentified files stay where they are until Search identifies them.
Originals go to `RomForge/.romforge-trash/<batch>/` (unless Keep originals)
until you undo; another copy outside of a game already in `Games/` goes
there too. Refresh also scans `Games/`. An import is a cleanup into
`Games/`, so everything below applies to it too.

Every version of a game is kept: regions, revisions, translations, hacks.
The library lists one row per game (grouped by the clone lists, the DAT's
parent game, else the title without tags; translations and hacks with
their base game), showing the original release from the first of your
**Preferred regions** (Settings; default USA, World, Europe, Japan), never
a translation or hack. ▸ expands it to its other versions under headings
that fold on their own: **Other regions**, **Editions** (special editions,
re-releases, demos and betas), **Translations**, **Hacks**, and
**Patches** (the patch files a scan found, as Translations and Hacks). A
game you have only translations, hacks or patches of is still listed by
its original, marked *not in your library*; a patch file for no game you
have is listed under the DAT game its readme names (or on its own). Its
Details offer **Apply…** when you have the game it is for. `romforge-cli
games` prints the same tree. Move the folder from Settings (**Move…**) or by hand: when
romforge opens it in a new place it rewrites the paths it stores. With a
RomForge folder, Organize hands over to Import (its earlier batches can
still be undone).

## DOS, ScummVM and Japanese computers (milestone 14)

```bash
romforge-cli dat download DOS          # TOSEC's PC disks + libretro's DOS.dat
romforge-cli search name f:12 --system DOS --title "Commander Keen 1 (1990)"
romforge-cli scummvm read              # ScummVM's own game list, from your ScummVM
romforge-cli scummvm import list.dat   # or one you saved: scummvm --dump-all-detection-entries
```

DOS and ScummVM games are folders of many files, so a folder is one game:

- **DOS.** libretro's `DOS.dat` knows a game only by its program's size
  and CRC32, so a hit is a suggestion ("Likely: CRC match" in Search),
  never a match. Once you confirm it, the folder around the program (or
  the zip it is in) is the game, and the other files in it join its row.
  A program loose in a scanned folder isn't taken as a folder game: put
  each game in its own folder. Import packs the whole folder into
  `Games/DOS/<Title>.zip`; the folder goes to the trash in one piece. Play
  uses the `dosbox_pure` core (zip or image as is), else DOSBox Staging or
  DOSBox-X with the game's program. TOSEC disk images still verify by
  hash as before. Enrich takes genre, year and developer from `DOS.dat`
  (by the program's CRC32, whatever you named it) and box art from
  libretro.
- **ScummVM.** romforge asks the installed ScummVM (on `PATH` or the
  Flatpak) for its detection list once (Settings → **Read ScummVM's
  list**) and keeps it in `Databases/`; it is ScummVM's data (GPL), made
  on your machine and never shipped. A folder is a game when every file
  an entry lists is in it with that size and the MD5 of its first (or
  last) bytes, as ScummVM decides; such a game is "identified by
  ScummVM's list", never verified. The list leaves out the engines with
  their own detection (SCUMM: LucasArts and Humongous; SCI: Sierra; Beneath
  a Steel Sky; interactive fiction), so the same button also asks
  ScummVM itself (`--detect --recursive`) about every scanned folder;
  what it names is remembered with a fingerprint of the folder's files.
  Games are titled by libretro's `ScummVM.dat` name for their game id
  (with the version), which brings its box art, description, developer
  and publisher. Import copies the folder to `Games/ScummVM/<Title>/` with
  a `<id>.scummvm` file for RetroArch's core; Play uses the `scummvm` core
  or ScummVM itself (`--auto-detect`).
  A game still in the zip or 7z it came in is found by the list too (only
  the archive's index and the few bytes the MD5s cover are read, nothing is
  unpacked) and shows as one game, its files no longer listed as unknown.
  ScummVM can't play from an archive, so it isn't playable until Rename &
  Cleanup unpacks it into its folder (each file read back and checked, the
  archive to the trash for undo; in place, beside the archive). ScummVM's
  own `--detect` only looks at folders.
- **Disk sets.** When every disk of a set ("Disk 1 of 4", "Disc 2") is
  imported, a `<Set>.m3u` lists them next to the files, and Play starts
  the playlist so the emulator can swap disks. Undo removes it. The floppy
  computers' cores open a set's disks themselves and read no zips, so for
  PC-88, PC-98, X1 and FM-7 a set is kept unzipped in
  `Games/<System>/<Set>/` with the playlist its core reads: `.m3u` for
  quasi88 (PC-88), `.cmd` for np2kai (PC-98), none for X1 and FM-7 (swap
  disks from the emulator's menu). A single disk is still one zip.
- **Japanese computers.** PC-88 gets the `quasi88` core, PC-98 more disk
  formats and libretro's own PC-98 DAT, Sharp X1 the `x1` core; FM-7
  (TOSEC) and FM Towns start in MAME (`mame-fm7`, `mame-fmtowns`; MAME
  needs those machines' own ROMs).

## Arcade: MAME and FBNeo (milestone 17)

```bash
romforge-cli dat mame                          # your MAME's own list (mame -listxml)
romforge-cli dat download "MAME 2003-Plus" FBNeo
romforge-cli launch "Metal Slug" -e mame       # mame -rompath <folder> mslug
```

A set only runs in the emulator version its DAT belongs to, so there are
three arcade systems: **Arcade** (current MAME: the DAT is your installed
MAME's own `-listxml`, read on your machine, SHA1 for every ROM and disk;
**Read MAME's list** in the DATs view), **MAME 2003-Plus** (libretro's
per-ROM XML, for the `mame2003_plus` core) and **FBNeo** (FBNeo's DAT, for
the `fbneo` core). Arcade ROMs have no telling extensions: every member of
a zip or 7z named after a set is hashed (importing an arcade DAT marks
those zips to be read again). Merged, split and non-merged sets all work: a
game counts when all its ROMs are found, in its own zip, its parent's or
its BIOS's; MAME's devices aren't games, and `nodump` and optional ROMs
don't count. A disk (CHD in a folder named after the set) is matched by the
SHA1 the CHD keeps in its header, without decompressing.

FBNeo's DAT has only CRC32 per ROM, which never counts as a match (rule 8):
an FBNeo ROM is verified when the file's size and CRC32 fit it and its
SHA1 fits the same ROM in a MAME DAT you have; otherwise it stays a
suggestion. The list shows each set by its DAT's description ("Metal Slug -
Super Vehicle-001"; the set name in Details), clones grouped under their
parent. Import copies the zips whole under their own names (sets are found
by name) into `Games/<System>/`, the parent's and BIOS's zips once, CHD
folders along; nothing is repacked. Box art comes from libretro's MAME and
FBNeo thumbnails, by description.

## Patches: translations and hacks (milestone 19)

```bash
romforge-cli patch "Mother 3"                                   # patches found for it; where to look
romforge-cli patch "Mother 3" ~/Downloads/mother3_1.3.zip       # what it would do
romforge-cli patch "Mother 3" ~/Downloads/mother3_1.3.zip --tag "English v1.3" --apply
```

Apply a translation or hack you downloaded to a game you have: romforge
makes a **new copy** and never touches the original. It doesn't download
patches itself (romhacking.net is read-only, its archive is private, and
no site offers an API): Details → **Patch…** gives search links to
romhacking.net and Romhack Plaza, offers the patches found for the game,
or takes one you pick: IPS, BPS, UPS, PPF or xdelta (DJW, FGK and LZMA
compression too), or a zip holding one (its readme is kept with the copy).

- **Patches join their game.** A scan lists the patch files it finds
  (loose, or the one patch in a zip) under the game they are for: a BPS or
  UPS by the ROM it was made from (size and CRC32), an xdelta by the file
  it names, any patch by a game's file of the same name beside it, by its
  name or its readme's title, or by the game its readme names. Whether it
  is a translation or a hack is read from its name and readme (you can
  change it). Details → **Patch…** shows them, ready to apply.
- **Patched ROMs are named on their own.** A ROM no DAT knows, which a
  patch beside it (same name, or a BPS/UPS's target size and CRC32) turns
  your verified copy of its game into exactly, is named "<Game>
  [Translation: <tag>]" (or Hack) in the game's group with no click:
  named by romforge, never verified. Forget it and it stays unknown.

- **Cartridges** are patched in memory, in the form the patch wants: BPS
  and UPS say it (their source's size and CRC32; one made for another
  revision is refused, naming what it wants), an IPS goes on the DAT's
  form (no copier header) unless its name says headered; a header taken
  off goes back on. Written as a zip like any import. In a game of several
  ROMs, a BPS or UPS goes on the one it was made from, others on the one
  they are named after, else (said in a note) the biggest; the rest come
  through under the DAT's names.
- **Discs** (xdelta, PPF) are patched on the track they were made from,
  streamed, the other tracks kept, and written as a CHD: the track an
  xdelta names, else the first that takes it (a PPF's check block and an
  xdelta's checksums refuse the others). A CHD is unpacked first, with a
  cue sheet from its own track list so pregaps stay right.
- The copy goes into `Games/<System>/` as "<Game> [Translation: <tag>]"
  (or Hack; `:` becomes `-` in the file name), lists as a version of the
  game, says which patch made it, and is "identified by you", never
  verified, unless a hack DAT lists it. `cleanup undo` removes it.

## Cleanup and rename (dry run, apply, undo)

```bash
romforge-cli cleanup plan -v      # changes nothing
romforge-cli cleanup apply
romforge-cli cleanup undo         # latest batch, or --batch N
romforge-cli cleanup history
romforge-cli cleanup empty-trash --older-than 30          # shows what would go
romforge-cli cleanup empty-trash --older-than 30 --yes    # deletes it for good
```

Every game in the Source folder that is verified by hash (or identified by
you, as a game or as a translation / hack of one) becomes one file named by its DAT, in `<Library>/<system>/` (the
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
originals back. Refresh never scans `.romforge-trash`. **Empty trash** (in
the Import / Cleanup dialog, or `cleanup empty-trash`) deletes those
originals for good, all of them or only batches older than N days; those
batches can't be undone after, and their new files stay.

Every zip romforge writes is deterministic: no clock (1980-01-01), the same
permissions and compression for every member, members in lower-case name
order. The same game always gives the same bytes, wherever it came from,
so a re-check or a sync compares exactly.

A scan lists the files it skipped for their extension, by kind (`.txt 5,
.nfo 3, …`), so a format romforge doesn't know is visible.

## Desktop app

Needs Node and the [Tauri Linux dependencies](https://tauri.app/start/prerequisites/)
(`webkit2gtk-4.1`). It uses the same database as the CLI. The layout follows
tinyMediaManager: a toolbar, platforms down the left, one row per game with columns
for what has been gathered (identified, metadata, box art, title screen,
snap), and a detail panel with Details, Development, Series, Versions and
Links tabs.

The toolbar:

- **Refresh** rescans the folders Settings names (Source and Library) and
  the RomForge folder's Games, not folders you scanned once; greyed out
  until there is one.
- **Scan folder ▾** scans any folder you pick without making it a setting,
  except the very first folder you scan while no Source is set, which
  becomes your Source. **Scan folder [dry run]** reads the folder on a
  throwaway copy of the library and lists what it holds (verified,
  candidate, unknown; what's new), then scans it for real or not as you
  say.
- **Search**, **Enrich**, **Settings** (in tabs: Folders, Rename &
  Cleanup, Display, Sources, Credits), **Play**.

Search, Enrich, Rename & Cleanup and Convert work on the games selected
in the list, never on the whole library (Refresh and Scan folder work on
folders). Click picks one game, Ctrl+click adds or drops one, Shift+click
picks a run; Shift with the arrows, Home, End, Page Up or Page Down
extends it, and Ctrl with them moves without picking (Ctrl+Space then
adds or drops). **Ctrl+A** selects everything listed (the platform and
filter in view), **Ctrl+Shift+A** nothing, **Ctrl+I** the others. A game
whose versions are folded takes them along. The foot of the list says how
many are selected.

**All games** and each platform list only games (verified, named by you,
translations and hacks, and the like); **Unidentified** collects
everything not matched yet, whatever its platform: unknown files, CRC32-only
candidates, files matched on another platform, and patches paired with no
game. Search works through them.

The sidebar shows each platform's icon (its name under it, wrapped, in
bold), fetched once into the cache; Settings → Display picks **Controller
icons** (the default) or **System icons** from RetroArch's own icon sets,
or **Platform logos** from Wikidata (Wikimedia Commons, on a light tile).
A platform without one keeps a coloured badge.
- **Rename & Cleanup ▾** does the whole job at once: every identified game
  renamed by its DAT, repacked (zip, or CHD for discs) and moved into
  RomForge (or the Library), or replaced in place, as Settings says.
  **Rename & Cleanup [dry run]** shows the plan first, with Undo and Empty
  trash.
- **Convert** turns identified disc images into CHD where they are, under
  their own names, for platforms whose emulators read CHD (never GameCube,
  Wii or Xbox): it shows what it would do first, every CHD is checked
  against the disc's hashes, and the originals go as Settings says (to the
  trash, or kept). `romforge-cli convert [--apply]`. **Versions** lists every version of the game in your library
(other regions, editions, translations, hacks, patch files) under the same
headings as the list, as a list or as box art, 3D boxes or banners.
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

**Search ▾ Name as suggested…** takes the selected files' suggestions at
once: you see each file with its suggestion and tick which to take (only a
name match or better can be ticked). A file whose name says it is a
translation or hack ("(English v1.01)", "English v2.00", "(English)") is
named as one of its game. `romforge-cli search accept ROW...` does the
same. Suggestions also follow the other names a game goes by: with the
LaunchBox database downloaded, "Takeshi's Challenge" leads to the DAT's
"Takeshi no Chousenjou (Japan)", asked only when no DAT title is alike.
An unknown cue sheet and the tracks it names are one row, and an unknown
file shows the platform its extension points to ("SNES?") as a hint, never
a match.

**Enrich** also asks Wikidata (free, no account) for developer, publisher,
country, director, composer, the series in order, and IDs that make the
Links tab point at each game's own MobyGames, RetroAchievements, Wikipedia,
IGDB and HowLongToBeat page.

With keys in Settings, Enrich also fills art libretro lacks (fanart, logo,
icon, 3D box, and any missing box art or screenshots) from ScreenScraper,
then SteamGridDB, then IGDB, each only asked for what's still missing.
SteamGridDB's grids are never used as box art (they are Steam capsules),
and IGDB images come at their largest sizes.

**View** (beside the filter): the library as a **List**, or as pictures
only (no titles; hover for the name) of **Box art**, **3D box** (box art
where there is no 3D box) or **Banner** (the fanart, every banner the same
wide shape, the game's logo over it). Every box cell
is the same size, and each box is drawn at its platform's and region's
true shape (SNES and Famicom boxes lie down, Genesis and DVD cases stand
up, PlayStation jewel cases are square; `systems::box_aspect`), all the
same height on one baseline; a scan of another shape fits inside it
whole, never cropped. Cards keep the
list's order, sort (a sort bar above them) and fold-out versions; pictures
load from disk as they scroll into view. The view is remembered.

**Details** opens with the box art and, under it, the fanart (click the
fanart to pick another: a popup lists every fanart libretro, the LaunchBox
database and the keyed sources have for the game, previewed small; your
pick is kept even when Enrich refreshes). Beside them, like an
encyclopedia's infobox, a compact card with the logo and one row per
fact; in a narrow panel it comes under the art. Facts are pictures where
they can be: a genre icon each (its name on hover), the ESRB rating as a
badge, region flags for the edition, the community rating as stars, and
the developer's and publisher's logos once Enrich has fetched them. Then
the overview and Media (3D box, title screen, in-game, icon). Every image
keeps its own aspect ratio and is never blown up; hovering gives its size
in pixels and its source, and a click opens it full size (small
screenshots scaled by whole steps, pixels kept sharp). Kinds with no art
are hidden; Settings → "Show empty media boxes" shows a box for each.

**Game databases** (DATs view, or `romforge-cli databases`): OpenVGDB and
the LaunchBox Games Database are free to download but published without a
licence, so romforge doesn't include them. One click fetches them from their
makers onto your computer (LaunchBox is about 110 MB, updated daily), and
Enrich then uses them only to fill what libretro and Wikidata leave empty:
overviews, studios, genres, release dates, players, community ratings,
Wikipedia and video links, and LaunchBox's 3D boxes, logos and fanart.
Details shows where an overview came from.

**Development** groups what is known into Studios, Credits, Release and
Identification (status, DATs, Wikidata item), leaving out anything empty.
Studios and people Wikidata names are cards with the studio's logo or the
person's photo from Wikimedia Commons (fetched by Enrich as small
thumbnails, kept with the art; each links to its Commons page for its
author and licence, and the card to the Wikidata item).
**Series** lists each title once, with badges for the platforms you have it
on: the numbered entries (Wikidata's series order) apart from the other
games, or, without a series, the libretro franchise; games you have open
with a click.

**Sorting**: every list sorts by its column headings (click again to
reverse): the games (title, platform, release year, status; a game's
versions sort the same way under their headings), the series, the DATs and
their games, Organize's moves and Import's games. Each list remembers its
sort. `romforge-cli games --sort year --desc` does the same. **Links** separates the game's own pages (from Wikidata IDs)
from plain site searches. libretro's `???` for an unknown value is hidden,
and "Nintendo ???" shows as "Nintendo (unconfirmed)".

Big libraries stay quick: the list is rebuilt only when something it
shows changed (a fingerprint of those tables, checked after each write),
kept on disk beside the database (`romforge.db-view.json`) for the next
start, and sent to the window only when it changed; pictures load from
disk as they come into view. The picture views (Box art, 3D box, Banner)
load thumbnails at the size they draw, made on first view and kept in
`~/.cache/romforge/thumbs` (JPEG, or PNG for pictures with see-through
parts; made again when a picture changes, and safe to delete); Details
shows the full pictures.

The toolbar shows romforge's logo (Settings → Display picks synthwave,
pastel, gummy, space or sprinkles); the app's icon is the RF monogram
(`app/src-tauri/icons/romforge.svg`, and `romforge-small.svg` for 32 and
64 px; render the PNGs with `rsvg-convert`).

**Settings** (toolbar, or `romforge-cli settings`): a **Source** folder of
unprocessed ROMs, an optional **Library** folder that Rename & Cleanup
sorts processed games into, what Rename & Cleanup does with the
originals (**Move, then trash originals**, the default; **Move and keep
originals**, which needs a Library or the RomForge folder; **Replace
originals**: renamed and repacked where each game is, the original to the
trash, `--in-place true`), and the
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

- `crates/romforge-core` — all logic (library): `scan`, `hash`, `identify`,
  `dat`, `fetch`, `organize`, `cleanup`, `enrich`, `disc`, `launch`, `view`,
  `search`, `suggest`, `names`, `titles`, `ident`, `patch`, `wikidata`,
  `extdb`, `artsources`, `home` (the RomForge folder), `layout` (every
  file and folder name romforge writes), `scummvm`, `patcher` (patched
  copies),
  `settings`, `report`, `db` (schema and migrations); one table of systems (`systems.rs`), one of copier
  headers and byte orders (`headers.rs`), one of game ID readers
  (`ident.rs`)
- `crates/romforge-cli` — thin CLI
- `app/` — Tauri 2 + Svelte shell
- `fixtures/` — tiny generated test files and DATs. Regenerate with
  `cargo test -p romforge-core --test fixtures -- --ignored regenerate`.

## License

MIT (see [LICENSE](LICENSE)). The systems table is adapted from
[remustwo](https://github.com/asafelobotomy/remustwo) (MIT). The DJW and
FGK decoders in `vcdiff.rs` are ported from
[xdelta3](https://github.com/jmacd/xdelta) (Copyright Joshua MacDonald,
[Apache-2.0](http://www.apache.org/licenses/LICENSE-2.0)). The genre icons
in Details and the sidebar's All games and Unidentified icons
(`app/src/gameIcons.js`) are from
[game-icons.net](https://game-icons.net) by Lorc, Delapouite, Skoll and Sbed,
licensed [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/), with
their background removed and recoloured; the app lists them under
Settings → Credits. The sidebar's controller and system icons are
[RetroArch's assets](https://github.com/libretro/retroarch-assets) (the
libretro team and contributors,
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)), and its
platform logos come from Wikimedia Commons through Wikidata (each under
its own Commons terms; many are trademarks); all downloaded by the app,
never shipped.
