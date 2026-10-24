# romforge guide

Everything the [README](README.md) leaves out: the command-line tool, how
romforge identifies and moves files, every part of the desktop app, the
theme format, and how to build it. [PLAN.md](PLAN.md) has the milestones
and what is still open.

- [Command line: quick start](#command-line-quick-start)
- [How identification works](#how-identification-works)
- [Organize](#organize-dry-run-apply-undo) · [Metadata and art](#metadata-and-art) · [Launch](#launch) · [Emulators](#emulator-settings-cores-and-installs)
- [The RomForge folder](#the-romforge-folder) · [BIOS files](#bios-files) · [Manuals](#manuals)
- [DOS, ScummVM and Japanese computers](#dos-scummvm-and-japanese-computers) · [Arcade](#arcade-mame-and-fbneo) · [Patches](#patches-translations-and-hacks)
- [Cleanup and rename](#cleanup-and-rename-dry-run-apply-undo)
- [Desktop app](#desktop-app) · [Fullscreen themes](#fullscreen-themes) · [Community themes](#community-themes) · [Steam and the Steam Deck](#steam-and-the-steam-deck)
- [Packages and building](#packages-and-building) · [Source layout](#source-layout) · [Licences and credits](#licences-and-credits)

Scanning and identifying only read your files. Only the apply commands
write: `organize apply` moves and renames them; `cleanup apply`, `import
--apply` and `convert --apply` repack them; `patch --apply` writes a
patched copy and `firmware import --apply` copies BIOS files. Each shows
its plan first (without `--apply`) and is journaled for undo.

## Command line: quick start

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
scanned 17 entries (19.8 KiB) in …/fixtures: 14 matched, 1 candidate, 1 unknown, 0 wrong_system, 1 duplicate, 0 manual, 0 sheet; skipped 14 non-ROM files (.dat 8, .xdelta 2, .mkv 1, .nfo 1, .txt 1, .url 1); skipped 1 symlinked folder
```

Only files whose extension some system uses (the `extensions` column in
`crates/romforge-core/src/systems/table.rs`) are hashed, plus `.zip`/`.7z`; the
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
`--db` or `ROMFORGE_DB` the database is the RomForge folder's (once `home
create` made one), else `$XDG_DATA_HOME/romforge/romforge.db`
(`~/.local/share/romforge/romforge.db`).

Rescans are incremental: a file whose size and modification time haven't
changed since it was hashed keeps its hashes (`--rehash` reads everything
again). `romforge-cli games` lists games the way the app does.

## How identification works

- Every file, and every member of a `.zip` or `.7z`, is hashed as a stream
  (CRC32, MD5, SHA1). Archives are never unpacked to disk.
- A `.chd` is read as its tracks: each CD track is rebuilt as the Redump
  `.bin` it came from and hashed as a member; a DVD CHD as one `.iso`.
- A Z3DS file (Azahar's compressed 3DS games: `.zcci`, `.zcia`, `.zcxi`,
  `.z3dsx`) is hashed as the file it holds, its zstd stream read straight
  through, so it matches the DAT like a zip member.
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

For the games you have, in house rule 10's order (each source fills only
what the ones before it left; `enrich::fields` keeps the order):

1. [libretro-database](https://github.com/libretro/libretro-database), by
   DAT name or CRC: genre, developer, publisher, franchise, release date,
   players, ESRB, serial, origin, rumble and analog support, Edge and
   Famitsu scores; and [thumbnails.libretro.com](https://thumbnails.libretro.com):
   box art, title screen, snap and clear logo (another version's picture,
   best of your Preferred regions first, when the game's own is missing).
2. Wikidata (CC0), and Wikipedia (CC BY-SA 4.0) through the article
   Wikidata links: see below. For arcade sets (which Wikidata doesn't
   list) the [Arcade Database](https://adb.arcadeitalia.net) by
   motoschifo, by set name: snap, title, marquee, flyer and cabinet, genre,
   players, year, manufacturer, series, and the history from
   arcade-history.com (shown with its copyright line); it takes one
   request at a time, which romforge keeps to. For GameCube, Wii, DS and
   3DS games, [GameTDB](https://www.gametdb.com), by the ID in the game's
   header: covers larger than libretro's, the whole cover, the 3D box and
   the disc or cartridge label (3DS: no 3D box). Both are free, without a key, and come
   before libretro's thumbnails.
3. With keys, ScreenScraper (by the ROM's hashes), then SteamGridDB and
   IGDB (by title).
4. The databases you downloaded: OpenVGDB, then LaunchBox.

With Settings → "Download video snaps" (off by default; 2–30 MB each),
Enrich also fetches a short gameplay clip: the Arcade Database's for
arcade sets, ScreenScraper's (with its keys) for the rest. It plays in
Details (click it) and, muted, behind a game on the fullscreen mode's
game page once the focus rests there.

Details names where each field came from. Art goes into the RomForge
folder's `Media/<System>/<kind>/` when there is one, else under
`$ROMFORGE_CACHE/art` (default `~/.cache/romforge`); a second run asks for
nothing new, except what a newer romforge learnt to ask for
(`enrich::ASK_AGAIN_SINCE`). `--refresh` fetches again.

## Launch

```bash
romforge-cli emulator list --all
romforge-cli launch "Super Mario World" --dry-run
romforge-cli launch "Silent Hill" -e duckstation
romforge-cli emulator add bsnes --system SNES --program bsnes --args "--fullscreen {rom}"
```

RetroArch (on `PATH` or the Flatpak) with the system's first installed core,
else a known standalone emulator (DuckStation, PCSX2, Dolphin, PPSSPP,
Flycast, mGBA, melonDS, xemu, RPCS3, Mednafen, Cemu, Azahar, Lime3DS, Citra, DOSBox Staging,
DOSBox-X, ScummVM, MAME; `PATH` or Flatpak). A Flatpak emulator is given
the game's folder for that run (`--filesystem`), since it may see only
your home folder, and the RomForge folder too when romforge sets the
emulator up from it (its config there) or passes it its BIOS folder
(`-rompath`, `--extrapath`, …: only when `Bios/<emulator>/` holds a file,
so an empty one never overrides the emulator's own paths). An emulator you
add gets `{bios}` as `Bios/<its name>/`.
Emulators you add come first. A ROM in a zip/7z goes to RetroArch as the
archive; standalone emulators only get archives if they read them
(`--archives`), since romforge never extracts. A disc starts from its sheet.
A 3DS CIA (`.cia`, `.zcia`) is an installer: the 3DS emulators install it
into their NAND (File → Install CIA) rather than starting it, so Play
refuses one and says so; cartridge dumps (`.3ds`, `.cci`, `.cxi`,
`.3dsx`) start. Azahar (current) plays decrypted games only, and is the
only one that reads Z3DS (`.zcci`…). Encrypted dumps, the ones No-Intro's
DATs match, play in its discontinued predecessors, Lime3DS 2119.1 and
Citra's last build, and in RetroArch's Citra core (libretro's, still
built), with `aes_keys.txt` (and `boot9.bin`, `seeddb.bin` for some
games) on the BIOS page; romforge never downloads those two programs, it
uses yours (PATH, Flatpak or AppImage). A decrypted dump doesn't match
No-Intro by hash; Search names it by the product code in its header
(`CTR-P-AREE`), as "identified by you", never verified.

## Emulator settings, cores and installs

```bash
romforge-cli emulator settings                         # global and per-emulator values
romforge-cli emulator set global fullscreen false
romforge-cli emulator set duckstation program /opt/DuckStation.AppImage
romforge-cli emulator cores --refresh                  # the cores romforge's platforms use
romforge-cli emulator get-core snes9x                  # into RomForge/Emulators/cores/
romforge-cli emulator remove-core snes9x --yes
romforge-cli emulator install duckstation [--apply]    # from Flathub
romforge-cli emulator import-setup pcsx2 [--apply | --undo]
```

**Emulator Settings** (toolbar) has a Global tab for every emulator at
once (fullscreen, game pad hotkeys) and a tab per installed emulator,
which takes the global value unless it sets its own, plus its own
**Program**, extra arguments and config settings (RetroArch's video
driver, VSync, ...). RetroArch's tab lists its cores: where
each is installed, its licence (non-commercial ones are flagged and asked
about), and what the libretro buildbot has; a download goes into
`RomForge/Emulators/cores/`, which romforge looks in first. An emulator
that isn't installed can be installed from Flathub into your own Flatpak
installation (`flatpak install --user`, no password). **Import my existing
setup** copies what an emulator keeps in its usual folders (memory cards,
saves, controller mappings, settings) into romforge's copy for it; undo
takes it back. romforge never ships an emulator or a core.

## The RomForge folder

Everything romforge keeps can live in one portable folder, always named
`RomForge`, inside a folder you pick:

```
RomForge/
  romforge.db                the library
  Games/<System>/<Title>.zip cartridges (art and details inside, in romforge/)
  Games/<System>/<Title>.chd discs, with a folder of the same name beside it
  Media/<System>/<kind>/     art (and manual/ for manuals)
  Databases/                 OpenVGDB, LaunchBox, ScummVM's list (made or downloaded by you)
  Emulators/                 AppImages you put there; cores/ (downloaded cores)
  Emulators/config/<emu>/    romforge's own copy of each emulator's config
  Bios/                      BIOS files (Bios/RetroArch/ is RetroArch's system folder)
  Saves/<System>/            SRAM and memory cards
  States/<System>/           save states
  Screenshots/<System>/
  Themes/<id>/               your fullscreen themes
  .romforge                  the marker: this folder is a RomForge folder
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

## BIOS files

```bash
romforge-cli firmware download                 # RetroBIOS's list of what each emulator loads
romforge-cli firmware check [--system "PlayStation"] [--all]
romforge-cli firmware import ~/bios ~/bios.zip  # the plan: what goes where
romforge-cli firmware import ~/bios --apply     # copies; never replaces a file
romforge-cli firmware undo --yes                # takes the last import back
romforge-cli firmware install-ps3 [--apply]     # Bios/rpcs3/PS3UPDAT.PUP into RPCS3
```

The **BIOS** page (at the foot of the platform list) shows, per system,
the files its emulators load, from [RetroBIOS](https://abdess.github.io/retrobios)'s
list (MIT; only the list is fetched, never a BIOS file), against what is in
`RomForge/Bios/`: `Bios/RetroArch/` for RetroArch's cores, `Bios/<emulator>/`
for the others. A file is *accepted* when its size and SHA1 or MD5 are
ones the emulator's code accepts; that says the emulator takes it, not
that it is a verified dump. A file with no hash in the list can only be
*present*. Drop folders, files, zips or 7z on the page (or `firmware
import`): each file is matched by its contents and copied where every
emulator that wants it looks; a file whose name fits but whose hash isn't
known is placed only when you tick it (`--also NAME`); so is a dropped
zip named like a MAME machine's set (`fm7.zip`), offered as it is.

Each emulator is pointed at its folder when it starts: RetroArch's system
folder; DuckStation, PCSX2, mGBA, Mednafen, melonDS and xemu through
romforge's config of theirs; Flycast by `FLYCAST_BIOS_PATH`; Dolphin's and
Cemu's files (`keys.txt`, `otp.bin`, `seeprom.bin`) linked into their data
folder; Azahar's, Lime3DS's and Citra's `sysdata/` files (`keys.txt` or
`aes_keys.txt`, `boot9.bin`, `shared_font.bin`, `seeddb.bin`, `otp.bin`)
linked into their data folder, and their NAND and SD card set to your own
data of that emulator or an older one (Azahar reads Lime3DS's and Citra's,
Lime3DS Citra's; XDG or Flatpak) so installed titles and saves stay where
they are, else to `Saves/Nintendo 3DS/` (with `Bios/<emulator>/nand/…`
linked in, never into yours); the Azahar and Citra cores' files
(`Bios/RetroArch/Azahar/`, `Citra/`) linked into `Saves/Nintendo 3DS/`,
where those cores keep their system folder; MAME's machines by `-rompath` (FM-7 and FM Towns need their
machine's set, `fm7.zip`, `fmtowns.zip`, in `Bios/mame/`); ScummVM by
`--extrapath`; DOSBox Staging's MT-32, Sound Canvas and SoundFont folders,
and DOSBox-X's MT-32 folder, by their settings on the command line. An
emulator is *ready* when every file it can't run without is accepted or
present; files only some games load don't decide it: a sub-system's
(Satellaview, DSi, as the systems table names them) and a cartridge
chip's (bsnes's `*.program.rom`), shown as "only Satellaview games",
"only games with that chip".

## Manuals

```bash
romforge-cli manual show "Super Metroid"       # yours, or whether Vimm's Lair has one
romforge-cli manual find "Super Metroid"       # archive.org's scans, numbered
romforge-cli manual get "Super Metroid" --pick 2
romforge-cli manual download "Super Metroid" --dpi 150   # Vimm's Lair's scan
romforge-cli manual add "Super Metroid" ~/manual.pdf
romforge-cli manual remove "Super Metroid" --yes
```

A game's **Manual** tab finds its manual: a PDF of yours (picked or
dropped on the tab), archive.org's scans under the game's name
(retrokit-manuals, Kirkland's Manual Labor), ScreenScraper's (seen by
Enrich, with its keys), the Arcade Database's for an arcade set (asked
when the tab opens), or Vimm's Lair's scan, found through Hasheous by the
game's hash. Each is downloaded only when you
click it, and kept as `Media/<System>/manual/<Title>.pdf` (the art cache
without a RomForge folder). Manuals are copyrighted scans; romforge ships
none. The reader (pdf.js) shows a page or a spread, fit to width or page,
with zoom, thumbnails, search, and highlights and notes kept for the game.
A scanned manual can be read by OCR (Tesseract, in the app); a language's
file is downloaded from Tesseract's `tessdata_fast` when first used, into
`~/.cache/romforge/ocr/`, and what it reads is kept for search.

## DOS, ScummVM and Japanese computers

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
  the playlist so the emulator can swap disks. Undo sets it aside with the rest. The floppy
  computers' cores open a set's disks themselves and read no zips, so for
  PC-88, PC-98, X1 and FM-7 a set is kept unzipped in
  `Games/<System>/<Set>/` with the playlist its core reads: `.m3u` for
  quasi88 (PC-88), `.cmd` for np2kai (PC-98), none for X1 and FM-7 (swap
  disks from the emulator's menu). A single disk is still one zip.
- **Japanese computers.** PC-88 gets the `quasi88` core, PC-98 more disk
  formats and libretro's own PC-98 DAT, Sharp X1 the `x1` core; FM-7
  (TOSEC) and FM Towns start in MAME (`mame-fm7`, `mame-fmtowns`; MAME
  needs those machines' own ROMs).

## Arcade: MAME and FBNeo

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

## Patches: translations and hacks

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
  verified, unless a hack DAT lists it. `cleanup undo` sets it aside in the trash.

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
7z members, and big zip members packed other than stored or deflated, are
unpacked, into the staging folder. Each
new file is read back and every ROM re-hashed before anything else
happens, and nothing is ever overwritten. Without Keep originals, the old
files (and other verified copies in Source) then move to `.romforge-trash/`
next to them, but only when everything in them was repacked; an archive
holding anything else stays, as do originals that another game not in
the batch still needs (an arcade BIOS or parent set). `undo` takes the
newest batch back: its new files go to the trash (`undo-<batch>/`, until
the trash is emptied) and the originals return. Batches are undone newest
first, and none before an emptied batch can be. Refresh never scans `.romforge-trash`. **Empty trash** (in
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
(`webkit2gtk-4.1`), and libudev for game pads (`libudev-dev` on Debian and
Ubuntu, `systemd-libs` on Arch). CI installs `libwebkit2gtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev libudev-dev`. It uses the same
database as the CLI. The layout follows tinyMediaManager: a toolbar,
platforms down the left, one row per game with columns for what has been
gathered (identified, metadata, box art, title screen, snap), and a detail
panel with Details, Manual, Development, Series, Versions and Links tabs.

The toolbar:

- **Refresh** rescans the folders Settings names (Source and Library) and
  the RomForge folder's Games, not folders you scanned once; greyed out
  until there is one. First it brings the lists games are named by up to
  date: ScummVM's detection list and MAME's own DAT when the installed
  ScummVM or MAME changed (or the list is missing; MAME only once you have
  Arcade), and the DATs, serials and clone lists of the systems you have
  files for once they are older than Settings → **Online lists on
  Refresh** (every 7, 14 or 30 days, the default, or never). A system the scan finds for the first time
  gets its lists in the same Refresh. ScummVM games are found before
  hashing, so their data files aren't hashed one by one
  (`romforge-cli refresh --no-lists` only rescans).
- **Scan folder ▾** scans any folder you pick without making it a setting,
  except the very first folder you scan while no Source is set, which
  becomes your Source. **Scan folder [dry run]** reads the folder on a
  throwaway copy of the library and lists what it holds (verified,
  candidate, unknown; what's new), then scans it for real or not as you
  say.
- **Rename & Cleanup** shows the plan first (Undo and Empty trash are
  there too). **Rename & Cleanup [apply now]** on the ▾ menu does the
  whole job at once: every identified game renamed by its DAT, repacked
  (zip, or CHD for discs) and moved into RomForge (or the Library), or
  replaced in place, as Settings says.
- **Convert** turns identified disc images into CHD where they are, under
  their own names, for platforms whose emulators read CHD (never GameCube,
  Wii or Xbox): it shows what it would do first, every CHD is checked
  against the disc's hashes, and the originals go as Settings says (to the
  trash, or kept). `romforge-cli convert [--apply]`.
- **Search**, **Enrich**, **Settings** (in tabs: Folders, Rename &
  Cleanup, Display, Sources, Steam, Credits), **Emulator Settings** (see
  [Emulators](#emulator-settings-cores-and-installs)), **Play**. Play shows a splash
  until the emulator is up; then romforge goes to the system tray
  (Settings → **While a game plays**: tray, minimise, or keep open). Its
  tray menu has **Show romforge** and **Stop**; when the game ends,
  romforge comes back by itself.
- **Fullscreen** (or F11) switches to the fullscreen mode, for a game pad
  and a TV: the platforms along the top (left/right), that platform's
  games as large box art below with the focused game's facts beside
  them, and a game's own page (A opens it, A again plays). Start opens
  the menu (sort, art, Back to desktop, Quit), Select more for a game,
  North (Y) its options; the shoulders page, the triggers jump; a help
  bar shows the buttons (or keys: Enter, Esc, Tab, Space, O, PgUp/PgDn).
  `romforge-app --fullscreen`, or Settings → **Start in the fullscreen
  mode**, opens it straight away. Start → **Theme** (or Settings →
  Display → **Fullscreen theme**) picks its look: SNES UK (the default,
  light or dark with the system), Famicom, Mega Drive or Game Boy, each
  with its colours, a grid or a list of games, a text size and the
  theme's own options. Changes apply at once and are kept.
- **Hotkeys** on the pad while a game runs, the same for every emulator
  (romforge reads them itself; the emulators' own pad hotkeys are off):
  Select+Start quits, Select+R1 saves a state, Select+L1 loads it, L3+R3
  opens the emulator's menu, Select+R2 held fast-forwards and Select+L2
  held rewinds. Hold L3 for two seconds and a ring of the one-shot
  actions (quit, save, load, the menu) shows over the game: point at one
  with the right stick and let go of L3 (or press A; B cancels). The ring
  dims what romforge can't do itself in that emulator (DuckStation's and
  PCSX2's own bindings answer the chords, not the ring). Quit works for every emulator; RetroArch,
  DuckStation and PCSX2 get the rest (PCSX2 has no rewind; rewind needs
  turning on: Emulator Settings → Rewind for RetroArch, DuckStation's own settings). Emulator Settings
  → **Hotkeys** lists them and what each installed emulator gets, and the
  Global tab's **Game pad hotkeys** turns them off. The ring draws over a
  fullscreen game where the desktop has layer-shell (KDE Plasma, wlroots)
  and `gtk-layer-shell` is installed; elsewhere it shows as a window of
  its own, which takes the focus while it is up.

Long jobs (scans, Refresh, Enrich, Rename & Cleanup, Convert, DAT and
database downloads, naming in bulk, patching, installs) wait their turn
behind the one running. The bar at the foot of the window shows how far
the whole queue is; click it for the job running now (its own bar, and
**Stop**, which halts it at the next safe point and keeps what is done)
and the ones waiting (**Remove**). Play and quick edits never wait.
Settings → **Threads for long jobs** sets how many files are hashed and
games repacked at once (automatic: one per CPU, at most 8; 1 for a slow
spinning disk); downloads use up to four, each website at its own pace
(`romforge-cli settings --threads N`).

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
Controller icons cover fewer platforms: the rest show romforge's own
icon in the same style where it has one
(`crates/romforge-core/assets/platform-icons/`), else their system icon. A logo is the Commons file the systems table names (where Wikidata
has none, or several), else the first of the platform's Wikidata items
that has one. A platform without one keeps a coloured badge. When the
table's icons or logos change, the kept ones are fetched again.
DATs, BIOS and Organize are at the foot of the platform list.

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
country, director, composer, the series in order, age ratings (PEGI, ESRB,
CERO, USK), release dates by region, the original title, and IDs that make
the Links tab point at each game's own MobyGames, RetroAchievements,
Wikipedia, IGDB and HowLongToBeat page. The overview comes from the
summary of the game's Wikipedia article, the one Wikidata links (never a
title search), in Settings' "Descriptions in" language when that Wikipedia
has it, else English; Details credits it ("from Wikipedia (CC BY-SA
4.0)", with a link to the article).

With keys in Settings, Enrich also fills what libretro lacks from
ScreenScraper, then SteamGridDB, then IGDB, each asked once per game:
pictures (fanart, logo, icon, 3D box, the box's spine, back and whole
cover, the cartridge or disc, the arcade marquee, and any missing box art
or screenshots), and ScreenScraper's synopsis (in the language set),
ratings, players, dates and genres, IGDB's summary and genres. ScreenScraper's
pictures come in the game's own region first, then Preferred regions'.
SteamGridDB's grids are never used as box art (they are Steam capsules),
its plain heroes come before blurred ones, largest first, IGDB images come
up to 1080 pixels high, and IGDB tells same-named games apart by release
year.

**View** (beside the filter): the library as a **List**, or as pictures
only (no titles; hover for the name) of **Box art**, **3D box** (box art
where there is no 3D box) or **Banner** (the fanart, every banner the same
wide shape, the game's logo over it). Every box cell
is the same size, and each box is drawn at its platform's and region's
true shape (SNES and Famicom boxes lie down, Genesis and DVD cases stand
up, jewel cases slightly wide, hinge included; one table, `systems::BOX_SIZES`, gives both `box_aspect` and `box_size`), all the
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
they can be: a genre icon each (its name on hover), age ratings as
badges (ESRB, PEGI, USK, CERO), Edge and Famitsu scores, region flags for the edition, the community rating as stars, and
the developer's and publisher's logos once Enrich has fetched them. Then
the overview and Media (3D box, box back, box spine, whole cover,
cartridge or disc, marquee, title screen, in-game, icon). Every image
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
Wikipedia and video links, and LaunchBox's 3D boxes, spines, backs, logos
and fanart.
Details shows where an overview came from.

```bash
romforge-cli databases list                     # what is downloaded, and when
romforge-cli databases download launchbox       # or openvgdb
romforge-cli databases import openvgdb ~/Downloads/openvgdb.zip   # one you downloaded
romforge-cli databases remove launchbox --yes
```

**Manual** finds, keeps and reads the game's manual; see [Manuals](#manuals).

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

**Versions** lists every version of the game in your library (other
regions, editions, translations, hacks, patch files) under the same
headings as the list, as a list or as box art, 3D boxes or banners.

**Links** separates the game's own pages (from Wikidata IDs) from plain
site searches. libretro's `???` for an unknown value is hidden, and
"Nintendo ???" shows as "Nintendo (unconfirmed)".

**Sorting**: every list sorts by its column headings (click again to
reverse): the games (title, platform, release year, status; a game's
versions sort the same way under their headings), the series, the DATs and
their games, Organize's moves and Import's games. Each list remembers its
sort. `romforge-cli games --sort year --desc` does the same.

Big libraries stay quick: the list is rebuilt only when something it
shows changed (a fingerprint of those tables, checked after each write),
kept on disk beside the database (`romforge.db-view.json`) for the next
start, and sent to the window only when it changed; pictures load from
disk as they come into view. The picture views (Box art, 3D box, Banner)
load thumbnails at the size they draw, made on first view and kept in
`~/.cache/romforge/thumbs` (JPEG, or PNG for pictures with see-through
parts; made again when a picture changes, and safe to delete); Details
shows the full pictures.

The toolbar shows romforge's wordmark, pixel lettering drawn as SVG
(`app/src/Wordmark.svelte`); the app's icon is "ROM" on an anvil, 32×32
pixel art (`app/src-tauri/icons/romforge.svg`; render the bundle's PNGs
from it with `rsvg-convert`).

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

## Fullscreen themes

A theme is data, never code: a folder with `theme.json` and `theme.css`
(and any pictures and fonts it uses), in `RomForge/Themes/<id>/` (or
`themes/<id>/` beside the library's database without a RomForge folder;
Settings → Display opens it). The folder's name is the theme's id.
Settings → Display lists a refused theme with the reason. While the
fullscreen mode is open, an edited theme reloads within a second.

```json
{
  "api": 1, "id": "mine", "name": "Mine", "author": "you", "licence": "CC0-1.0",
  "layout": "grid", "layouts": ["list"], "fallback": "title",
  "schemes": { "light": { "--fs-bg": "#eee" }, "dark": { "--fs-bg": "#111" } },
  "options": [{ "key": "corners", "label": "Card corners", "choices": [
    { "value": "round", "label": "Round", "vars": { "--fs-radius": "1vh" } },
    { "value": "square", "label": "Square", "vars": { "--fs-radius": "0" } }
  ] }]
}
```

- `layout`: `grid`, `list`, `cover-flow`, `wheel`, `shelves`, `hero`,
  `hero-bottom` or `bookshelf` (`layouts`: the others it allows). Cover flow is one row of
  box art turned in 3D; the wheel, the games' logos turning down the right;
  shelves, rows of the platform's games played lately, its favourites and
  all of them; the hero rows, a strip of small tiles along the top with
  the game's details under it (`hero-bottom`: along the bottom, the
  details over it). The wheel and hero rows show the game's fanart behind
  them whatever the details say. The bookshelf stands the games' boxes
  side by side on wooden shelves, spines out, each at its real size
  (`systems::box_size`: listed sizes, else worked out from its shape),
  with its spine art or one drawn in its platform's colour. Settings →
  Display (or Start → Bookshelf there) picks one endless shelf each or a
  bookcase of shelves of 30 (down goes to the next shelf, and moving on
  from a shelf's end goes on to the next's start), their order (name,
  release date either way, most played), All games' platforms on a shelf
  each or all mixed, and a favourites shelf at the top and a recently
  played one under it, and the game's details above the shelves or
  beside them, on the left or the right (the shelves then shorter, 15 to
  a bookcase shelf). The shelves hold the platform picked along the
  top, its favourites and recently played shelves its own. The focused
  box slides out, Accept takes it off the shelf in 3D (left and right
  turn it a quarter at a time to its spine, back and other side, its back
  art else one drawn from its screenshots and overview), and Accept again
  opens the game's page;
  `fallback`: what a game with no art shows, `title` or `platform` (its
  platform's icon).
- `schemes`: colour schemes, each a set of variables; with both `light`
  and `dark`, the system's choice is followed unless one is picked.
- `options`: each choice sets variables and the root attribute
  `data-fs-opt-<key>="<value>"` for the CSS to match.

**Theme API 1.** Variables (set by schemes and options, or in the CSS):
`--fs-bg`, `--fs-bg-2` (the background's glow), `--fs-panel`, `--fs-text`,
`--fs-muted`, `--fs-accent`, `--fs-accent-text`, `--fs-line`, `--fs-focus`,
`--fs-warn`, `--fs-shadow`, `--fs-scrim`, `--fs-radius`, `--fs-card-ratio`,
`--fs-font`, `--fs-title-font`, `--fs-gap`; for cover flow,
`--fs-flow-angle` (the side covers' turn, `55deg`), `--fs-flow-depth` (how
far back they stand, `14vh`), `--fs-flow-spread` (their step, `0.42` of a
cover), `--fs-flow-dim` (their brightness, `0.7`) and
`--fs-flow-perspective` (`110vh`); `--fs-wheel-curve` (how far the wheel's
far logos move out, `1.2vw`) and `--fs-hero-opacity` (the fanart's in the
wheel and hero rows, `0.5`); `--fs-shelf-wood` and `--fs-shelf-back` (the
bookshelf's board and back panel); `--fs-shelf-plate` and `--fs-shelf-plate-text` (the shelves' brass name plates, `#e8cf8a` and `#3a2a10`), `--fs-shelf-text` (the empty bookcase's words, `#f3e6d2`), `--fs-shelf-spine` (a box's colour when its platform has none, `#5c5c6a`); `--fs-overview-lines` (the overview's
lines, `6`; `2` under cover flow) and `--fs-backdrop-opacity` (`0.18`).
`--fs-scale` is the user's text size and only romforge sets it. Hooks: the
root `[data-fs-root]` (in the CSS, `:scope`) with `data-fs-screen` (`home`,
`game`), `data-fs-layout`, `data-fs-shelf-details` (`left`, `right`: the
bookshelf's details beside it), `data-fs-aspect` (`wide`, `standard`,
`tall`), `data-fs-theme`, `data-fs-scheme` and `data-fs-opt-*`; inside it
`data-fs-wordmark`, `data-fs-carousel`, `data-fs-platform`, `data-fs-grid`,
`data-fs-list`, `data-fs-flow`, `data-fs-wheel`, `data-fs-tiles`,
`data-fs-shelves` and its rows' `data-fs-shelf` (`recent`, `fav`, `all`),
`data-fs-bookshelf`, `data-fs-box` (a box off the shelf) and its faces'
`data-fs-face` (`front`, `back`, `spine`), `data-fs-card`, `data-fs-row`,
`data-fs-selected`, `data-fs-art="missing"`, `data-fs-title`,
`data-fs-info` and in it `data-fs-detail` (`logo`, `platform`, `released`,
`edition`, `genre`, `players`, `modes`, `developer`, `publisher`, `rating`,
`community`, `played`, `derived`, `overview`; `backdrop` is the root's
fanart), `data-fs-button`, `data-fs-menu`, `data-fs-help` and
`data-fs-focus` (where the focus is); `data-fs-fav` (a favourite's star),
`data-fs-search`, `data-fs-keyboard` and `data-fs-key` (the search screen),
`data-fs-attract` and `data-fs-screensaver`. The splashes outside the mode
carry `data-fs-splash` (`start`, `exit`) and `data-fs-platform`. Class
names are not part of the API and change.

The CSS is served wrapped in `@scope ([data-fs-root])`, so it styles the
fullscreen mode only, under a policy that loads nothing from outside
romforge. Refused: `@import`, URLs with a scheme or `//`, a `url()` out of
the theme's folder, escapes, unbalanced braces, more than 256 KB of CSS
or 8 MB in all, variables not named `--fs-…`.

### Extras in the fullscreen mode

- **Details of the game in focus** (Settings → Display, or Start →
  Details there): the logo (else the title), platform, release date,
  genre, players, modes, developer, publisher, age rating, community
  rating, region and edition, what a hack or translation was made from,
  play time, the overview, and its fanart dimmed behind the home screen;
  each on or off, the same in every layout. Publisher, modes, age rating,
  region and the fanart start off.
- **Favourites, Recently played, Most played:** X (or F) toggles a game's
  favourite; play time comes from the games romforge started (runs over
  10 s). These are kept per game in the library (`user_games`), apart from
  what Enrich gathers, so a refresh never touches them.
- **Search:** Start → Search, or `/` on a keyboard. The d-pad moves over
  the on-screen keys, A types, Y a space, B deletes (and leaves when the
  line is empty), Start goes to the results. Every word must be in a
  title; titles starting with what you typed come first.
- **Attract mode and screensaver** (Settings → Display; off by default):
  after 3, 5 or 10 minutes untouched, games are shown in turn with their
  fanart or box art; 10 or 30 minutes after that the screen dims. Any
  button wakes it, landing on the game it showed. `ROMFORGE_ATTRACT_SECONDS`
  overrides the time (for trying a theme).
- **Music** (Settings → Display): a folder of yours (MP3, Ogg, Opus, FLAC,
  M4A, WAV, and the folders in it) played while the fullscreen mode is
  open, shuffled unless turned off, paused while a game runs. Start →
  Music skips, pauses and sets the volume.
- **The emulator for a game:** its page's **Emulator…** (or Y → Emulator…)
  lists the installed ones for its platform; the pick is kept for that
  game, and if it can't start the game romforge's own choice is used.
- **Splashes:** the startup splash and a short "Back from …" one when a
  game ends (with the time played) carry the platform's colour.

### Community themes

Themes others made are in [romforge-themes](https://github.com/asafelobotomy/romforge-themes).
**Settings → Display → Get themes…** (or Start → Theme → **Get themes** in
the fullscreen mode) lists them with a preview, author, version, size and
licence; nothing is downloaded until you accept its licence. romforge
fetches each file the index lists, checks its size and SHA-256, checks the
whole theme as it checks yours, and only then puts it in your themes folder
(an older download is replaced; a failed one leaves it as it was). A
newer version in the index shows as **Update**. A theme of yours with the
same name is never touched, and **Remove** removes downloaded themes only.

```bash
romforge-cli themes list                              # yours, downloaded, built in
romforge-cli themes available                         # the index, and what you have
romforge-cli themes install neon-arcade --accept-licence
romforge-cli themes remove neon-arcade
romforge-cli themes index ~/romforge-themes [--check] # write / check its themes.json
```

The index is `themes.json` at the repository's root: each theme's id, name,
version, author, licence (and its URL), description, preview, and every
file's path, size and SHA-256, under `themes/<id>/`. `romforge-cli themes
index` writes it from the folders, refusing any theme romforge would refuse
(its CI runs it with `--check`). `ROMFORGE_THEMES_INDEX` points romforge at
another index: a URL, a `themes.json`, or a checkout's folder.

## Steam and the Steam Deck

romforge goes into Steam as one non-Steam game, started with
`--fullscreen`: games are picked inside romforge, which starts each
emulator as its own child (gamescope, Steam's Game Mode, loses windows a
launcher hands off). **Settings → Steam** shows what it would write first:

- the entry in `userdata/<account>/config/shortcuts.vdf` (your other
  shortcuts kept as they are; the file is backed up to
  `shortcuts.vdf.romforge-backup`), named romforge, starting the AppImage
  when romforge runs as one, else `romforge-app`;
- its art in `config/grid/` (portrait and wide capsules, hero, logo, icon),
  romforge's own;
- the **romforge (Deck)** template in `controller_base/templates/`: an Xbox
  pad (romforge reads the pad itself), the right trackpad as mouse, L4
  Select+Start (quit), R4 L3 (held: the ring of hotkeys), L5 Select, R5
  Start. Pick it in romforge's controller settings in Steam.

Steam rewrites its list of shortcuts when it closes, so nothing is written
while it runs. Moving romforge (a new AppImage) and adding it again moves
the one entry; **Remove from Steam** takes the entry, its art and the
template out again.

```bash
romforge-cli steam status
romforge-cli steam add --dry-run   # what it would write
romforge-cli steam add [--user ACCOUNT]
romforge-cli steam remove
```

`ROMFORGE_STEAM_ROOT` points romforge at another Steam folder (tests).

## Packages and building

Each [release](https://github.com/asafelobotomy/romforge/releases) has:

- `romforge_<version>_amd64.deb` (Debian, Ubuntu) and
  `romforge-<version>-1.x86_64.rpm` (Fedora, openSUSE): the app and
  `romforge-cli`, using the system's WebKitGTK; RetroArch, ScummVM,
  gtk-layer-shell and GStreamer's good and libav plugins (for music in
  the fullscreen mode) are recommended. The rpm's package names are
  Fedora's; on openSUSE some may differ (the tray's libayatana package
  included), so install those by hand if zypper can't find them;
- a `PKGBUILD` for `romforge-bin` (Arch, Manjaro; build it with
  `makepkg -si`), with the emulators as optional dependencies;
- `romforge_<version>_amd64.AppImage`: one portable file for any distro
  (bigger: it carries its own WebKitGTK; needs FUSE);
- `romforge-cli-<version>-x86_64-linux.tar.gz`: the CLI alone;
- `SHA256SUMS`.

There is no Flatpak: romforge starts the emulators installed on your
system and works with files on any drive, which a Flatpak's sandbox
doesn't allow. A release is made by pushing a tag `vX.Y.Z` matching the
version in `Cargo.toml`, `tauri.conf.json` and `app/package.json`
(`.github/workflows/release.yml`, which runs the tests first).
To build the packages yourself: `cd app && npm ci && npx tauri build`
(they land in `target/release/bundle/`).

## Source layout

- `crates/romforge-core` — all logic (library): `scan`, `hash`, `identify`,
  `dat`, `fetch`, `organize`, `cleanup`, `enrich`, `disc`, `launch`, `view`,
  `search`, `suggest`, `names`, `titles`, `ident`, `patch`, `wikidata`,
  `extdb`, `artsources`, `home` (the RomForge folder), `layout` (every
  file and folder name romforge writes), `scummvm`, `patcher` (patched
  copies), `vcdiff` (xdelta's extras), `settings`, `report`, `db` (schema
  and migrations), `library` (the `Library` methods the shells call),
  `job` and `parallel` (long jobs: progress, stop, threads), `thumbs`
  (picture-view thumbnails), `firmware` (BIOS files), `manual` (manuals
  and OCR words), `emusettings` (Emulator Settings), `obtain` (cores and
  Flathub installs), `controller` and `hotkeys` (game pads and the hotkey
  chords), `music`, `themes`, `user_games` (favourites, the emulator
  picked per game), `steam`; one table of systems (`systems/table.rs`),
  one of copier headers and byte orders (`headers.rs`), one of game ID
  readers (`ident.rs`), one of hotkeys (`hotkeys.rs`)
- `crates/romforge-cli` — thin CLI
- `app/` — Tauri 2 + Svelte shell
- `fixtures/` — tiny generated test files and DATs. Regenerate with
  `cargo test -p romforge-core --test fixtures -- --ignored regenerate`.

## Licences and credits

MIT (see [LICENSE](LICENSE)). The systems table is adapted from
[remustwo](https://github.com/asafelobotomy/remustwo) (MIT). The DJW and
FGK decoders in `vcdiff.rs` are ported from
[xdelta3](https://github.com/jmacd/xdelta) (Copyright Joshua MacDonald,
[Apache-2.0](http://www.apache.org/licenses/LICENSE-2.0)). The app bundles
[pdf.js](https://github.com/mozilla/pdf.js) (`pdfjs-dist`, Mozilla,
Apache-2.0) for the manual reader, and
[tesseract.js](https://github.com/naptha/tesseract.js) and
`tesseract.js-core` (Apache-2.0; Tesseract itself is Apache-2.0) for OCR;
Tesseract's language files (`tessdata_fast`, Apache-2.0) are downloaded
when used, never shipped. The genre icons
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
never shipped. Arcade games' pictures, videos, facts and manuals:
scraping using the [Arcade Database](https://adb.arcadeitalia.net) by
motoschifo, with its history text © arcade-history.com; GameCube, Wii,
DS and 3DS covers and labels from [GameTDB](https://www.gametdb.com); both
fetched per game, never shipped. romforge's own platform icons, in flatui's style for
platforms flatui lacks, are MIT like the rest, except ScummVM's: PC,
Tandy VIS, Sega Lindbergh and ScummVM drawn by hand (their SVG sources
beside them); the others generated with OpenAI's GPT Image
(`gpt-image-2.5-flare`), with three of RetroArch's flatui icons as the
style reference, and touched up by hand. The ScummVM icon is the
exception to MIT: its label carries the
[ScummVM Team](https://www.scummvm.org)'s logo, under the
[GPL](https://www.gnu.org/licenses/gpl-3.0.html).
