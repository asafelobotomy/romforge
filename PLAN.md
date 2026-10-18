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

6. **Layout** *(built; Details reorganized 2026-10-17: fanart, box +
   logo + facts, overview, Media; true aspect and size; empty boxes hidden
   unless Settings shows them)* — the layout above on the data romforge already has;
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
   Laid out after tinyMediaManager's search window (2026-10-17): a
   scraper (DAT titles offline, or Wikidata's entity search, plus the
   candidates Enrich kept) with search text and platform; a results table
   (title, year, platform, id, fit %), box art (art cache, else libretro's
   thumbnail) and details (DAT ROMs set against your file's size); "Not
   listed?" for your own title; chips for what to fetch for that game once
   a result is taken, and "Do not overwrite existing data". Later: the
   keyed sources (ScreenScraper, IGDB) as scrapers too.
9. **Deeper enrich** — Wikidata *(built)*: only the titles you have are
   looked up, 20 per SPARQL query, through Wikidata's own search run inside
   the query service (`haswbstatement` on the platform item and "video
   game"); listing every game on a platform got one query per run past the
   rate limit. A game is matched when exactly one candidate has the same
   title (narrowed by release year), ambiguous ones are kept for Search.
   Candidates and searched titles are kept per platform item, so a run
   stopped by a 429 resumes where it stopped and still fetches details for
   games matched before; `--refresh` searches again.
   Stores developer, publisher, country, director, composer, designer,
   genre, modes, the series with its ordinal and every member, and IDs for
   MobyGames, IGDB, HowLongToBeat, RetroAchievements and Wikipedia (direct
   links). Requests are 2 s apart; 429/503 answers are retried after
   Retry-After up to 30 s, beyond that the run stops asking and reports the
   wait. Platform items are in the `wikidata` column of
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
10. **Cleanup & rename** *(built)* — repack each game into one archive named by the
    owner's title preferences, with a `romforge/` folder inside holding the
    art and a `.romforge.md` / `.nfo` with its details, under the guardrails
    of house rule 7. Decided 2026-10-17: archives are named by the DAT name
    as is ("Super Mario World (USA).zip"); cartridge games are repacked as
    zip; disc games become CHD (CHD can't hold extra files) with art and
    details in a sidecar folder named like the game next to it.
    Built: hash-verified and identified-by-you games with a file in Source
    go to `<Library or Source>/<system>/`. ROMs stream from the old file or
    zip into the new one; only 7z members (one compressed stream) and zip
    members over 64 MiB are unpacked into staging. CD CHDs take `.bin`
    tracks (type from the first sector, pregaps from the game's own cue);
    DVD CHDs one `.iso`. Each new file is read back and re-hashed before
    anything else; originals (and other verified copies in Source) go to
    `.romforge-trash/<batch>/` next to them only when everything in them
    was written, never with Keep originals. Journaled; undo removes the new
    files and moves the originals back. Not yet: `.nfo` (only
    `.romforge.md`), GDI/Dreamcast and `.img`/`.ccd` discs, emptying the
    trash from the app, multi-disc `.m3u`.
11. **More free sources** (tier 1 of house rule 10: free, no account, no
    key; reviewed 2026-10-17), in this order:
    1. *libretro, more of it*: `Named_Logos` from the thumbnail server
       (free logos before any keyed source), and the `metadat` fields not
       fetched yet: `esrb`, `bbfc`, `elspa` (ratings), `origin` (country),
       `serial`, `rumble`, `analog`, `enhancement_hw` (e.g. Super FX) and
       `magazine` (review scores). Same code path as today.
    2. *Wikipedia summaries* (REST `page/summary`, no key, descriptive
       User-Agent) for games with no description: most of the library has
       none. Only for games whose article Wikidata names (no guessing).
       Text is CC BY-SA 4.0: shown and written to `.romforge.md` with
       "From Wikipedia" and a link. Never its thumbnail (usually a
       non-free cover).
    3. *More Wikidata properties* in the same details query: P154 logo
       image (Commons, free files only, licence checked per file), P444
       review scores with their source, ratings (P852 ESRB, P908 PEGI,
       P853 CERO, P914 USK), P1705 native title, P437 distribution format,
       regional release dates (P577 with place qualifiers), and IDs for
       more links: P4769 GameFAQs, P6783 speedrun.com.
    4. *What the DAT names already say* (offline): region, languages,
       revision, and flags such as Proto, Beta, Demo, Unl, parsed from
       No-Intro / Redump / TOSEC naming. Shown in Details and Search.
    5. *MAME software lists* (`hash/*.xml`, CC0-1.0) for consoles and
       handhelds, matched by SHA1 only (house rule 8): publisher, year,
       serial, release date, alternate (Japanese) title, cartridge hardware
       (PCB, chips, battery save) and parent/clone links, which 1G1R needs.
       Downloaded like DATs, read offline.
    Worth a look before use (free, terms or access unclear): Hasheous
    (AGPL, free public hash lookup that maps to IGDB and VIMM's Lair
    manuals; no account per its README), GameTDB (GameCube/Wii/DS/3DS XML,
    open wiki, no stated licence, covers unclear), Recalbox Game DB (no
    stated licence or API). Not used: RetroAchievements' hash library
    (needs a login token), Giant Bomb IDs on Wikidata (former scheme).

    *Downloaded by the user* *(built 2026-10-17)*: OpenVGDB and the
    LaunchBox Games Database are free downloads with no account but no
    licence or terms published (all rights reserved by default, and much of
    the text comes from box backs and GameFAQs). So romforge never ships
    them (AGENTS.md rule 10): DATs view → Game databases (or `romforge-cli
    databases download|import|list|remove`) fetches them from their makers
    into `databases/` next to the library. Enrich then fills only empty
    fields (and libretro's `???`): OpenVGDB by SHA1, then LaunchBox by exact
    title on the system's LaunchBox platforms (new `launchbox` column in
    `systems.rs`, accents folded, alternate names, exactly one game or
    nothing); LaunchBox images come after libretro and before the keyed
    sources, the game's region first. Each field and image keeps its source
    (`game_meta.source`, migration 13), shown in Details and noted in
    `.romforge.md`. Import takes about 8 s; the LaunchBox index is about
    280 MB. On a copy of the real library: descriptions for 22 of 22 games
    (from 3), and 21 each of 3D boxes, fanart and logos. Findings:
    - **OpenVGDB** v29 (Nov 2021, 9 MB zip, 42 MB SQLite). Tables ROMs
      (system, region, CRC/MD5/SHA1, size, DAT file name, parent, serial,
      language, dump source: No-Intro 30k, MAME 11k, Redump 11k) and
      RELEASES (title, region, front/back/cart/disc cover URLs,
      description, developer, publisher, genre, date, GameFAQs page).
      Apparently built by matching DAT entries to GameFAQs (each release
      links its GameFAQs page); covers hot-linked from
      GameFAQs or the maintainer's GitHub. Keyed by hash, so it fits
      house rule 8: 21 of the library's 28 hashed files found by SHA1, 18
      with a description. Not updated since 2021; no build scripts
      published.
    - **LaunchBox Games Database** (`Metadata.zip`, 108 MB, re-exported
      daily): Metadata.xml has Game (190k: name, platform, overview,
      developer, publisher, genres, release date/year, max players,
      cooperative, ESRB, community rating and count, release type, video
      and Wikipedia URLs, Steam id), GameAlternateName (71k, by region),
      GameImage (1.3M: GUID file name on images.launchbox-app.com, type,
      region, CRC32) and Platform (hardware specs). Files.xml maps 86k
      known file names (mostly DAT names) to game names; Mame.xml has
      MAME sets with flags (bootleg, prototype, mature, mahjong…), genre,
      play mode and clone-of. **No ROM hashes**: LaunchBox identifies by
      file name, then title + platform. Community-edited with moderator
      approval, exported nightly. Exact title + platform matched 21 of the
      22 games here (the miss: "Pokémon" with its accent), all with an
      overview and many art kinds.
    - **Lessons for romforge** (usable without their data): a finer art
      taxonomy (box back, spine, cart, disc, banner, title vs gameplay
      screenshot, clear logo, background), each image tagged with region
      and CRC32 to drop duplicates; alternate names per region; a known
      file name → title table like Files.xml to tidy names before title
      matching; folding accents ("Pokémon" = "Pokemon") in title matching;
      a nightly single-file export as a model if romforge ever shares its
      own data.
12. **Sharper identification** (researched 2026-10-18). Size + SHA1 or
    size + MD5 stays the only "verified" (house rule 8). Each step below
    either finds exact matches that are missed today, or gathers evidence
    for files that stay unknown, shown as a suggestion the user confirms
    (it then becomes "identified by you"). In this order:
    1. *Close the normalized-hash gaps* (small) *(built 2026-10-18)*.
       `headers.rs` rules can now reorder bytes as well as skip a header:
       - N64 `.v64` (16-bit swapped) and `.n64` (32-bit reversed) are
         recognized by their first word and also hashed as `.z64`;
       - Super Magic Drive `.smd` (magic AA BB, 16 KiB blocks of odd then
         even bytes) is de-interleaved after its 512-byte header.

       Same raw + stripped columns, one pass, no rewriting. Migration 14
       has `.v64`/`.n64`/`.smd` files hashed again on their next scan.
       Not needed after all:
       - the SNES `size % 1 KiB == 512` rule already strips PC Engine
         copier headers (their data is a multiple of 128 KiB);
       - RetroAchievements' `% 8 KiB` would only miss more;
       - NES 2.0 headers are the same 16 bytes as iNES.

       Cleanup still copies such files as they are (a `.v64` in a zip
       named `.z64`). Writing the DAT's byte order instead is a cleanup
       follow-up: the new file would then hash exactly as the DAT says.
    2. *Keep more of the DAT* (small): `cloneof`/`romof`, region and
       serial. libretro's `metadat/redump` mirror has a serial on each
       game and rom (checked: SLUS-00067 is Symphony of the Night, USA),
       and `metadat/serial` covers 35 cartridge systems. Both are CC BY-SA
       4.0 and downloaded like DATs. Parent/clone feeds 1G1R (replaces the
       parked idea below).
    3. *Read the game's own ID* (medium), with one table of readers per
       system, like `headers.rs`:
       - SNES internal title, region and checksum (LoROM or HiROM by the
         checksum complement);
       - Mega Drive serial and region (`0x180`, `0x1F0`), GBA game code
         (`0xAC`), GB title (`0x134`), N64 ID (`0x3B`), NDS code (`0x0C`);
       - PS1/PS2 `BOOT=` in `SYSTEM.CNF`, from the first few MB of track
         1 (on the real library: `SLUS_000.67`, `SLUS_007.07`,
         `SLUS_009.74`, each in seconds);
       - Saturn, Sega CD and Dreamcast IP.BIN, the GameCube/Wii 6-byte
         ID, and PSP `PARAM.SFO`.

       Stored per file. Never a match on its own, since revisions and
       discs share serials, but a filter and tie-breaker, and the system
       hint the parked "scan-time system hints" idea wanted.
    4. *Parse names* (small to medium). `shiratsu-naming` (MIT, 1.1.0)
       tokenizes No-Intro, TOSEC and GoodTools names into base title,
       region, languages, revision, dev status and flags (`[b]`, `[!]`,
       `[h]`, `[tr en]`, `[T+Eng]`). It serves 11.4 too, and reads the file
       names of unknown files.
    5. *A title index with vetoes* (medium), built per system from the
       DATs:
       - normalize articles, "&", accents and punctuation, turn Roman
         numerals into digits, and keep the subtitle as a field of its
         own;
       - hard tokens veto a match: numerals, years, and Jr./DX/Plus/
         Turbo/Special ("Street Fighter II" never matches "III");
       - score token-set similarity, built on `strsim` (MIT). Jaro-Winkler
         alone is unsafe ("II" vs "III" scores about 0.98).
    6. *Suggestions with evidence* (medium). Each unknown file gets scored
       suggestions, shown in the library and as the top three in the
       Search dialog, each with its evidence: "name exact, size equal,
       region Japan agrees (header), hash differs". The bands are never
       counted as verified:
       - *Likely, CRC match*;
       - *Likely, modified copy of X*: same size, the name agrees, the hash
         differs (bad dump or hack);
       - *Suggested, name match*;
       - *Possible, similar name*;
       - *Unknown*.

       Confirming a suggestion makes it "identified by you".
    7. *Hacks and translations* (medium). Strip patch tags ("(English
       v2.0)[Addendum]", `[T+Eng]`, `[tr en]`, `(Hack)`), match the base
       title, and label the file "translation of X" or "hack of X": a
       status of its own, never verified. Evidence comes from the internal
       header and libretro `metadat/hacks`. A BPS patch beside the ROM
       records the CRC32s of the source and the target, which proves the
       base. On the real library, "Dragon Quest III (English
       v2.0)[Addendum]":
       - matches one SNES DAT title, "Dragon Quest III - Soshite Densetsu
         e... (Japan)" (4 MiB, serial SHVC-AQ3J-JPN);
       - has internal title DRAGONQUEST3, region byte Japan and a valid
         checksum;
       - is 6 MiB, because the patch expands the ROM.
    8. *Cue sheets by content* (small). Compare a cue's tracks (file names,
       modes, indexes) with the DAT's cue instead of its bytes. A cue
       rewritten with CRLF or other spacing then shows as "cue fits", not
       as an unknown file. The library's Symphony of the Night cue is 252
       bytes against Redump's 282, with the same two tracks.
    9. *Containers* (large, last). RVZ/WIA/WBFS/CISO/GCZ store the disc's
       6-byte ID and size (RVZ: `iso_file_size`) but no hash of the
       original ISO. ID + size gives a suggestion at once; a strict match
       needs streaming decompression inside romforge (house rule 7).
       CSO/ZSO/PBP work the same way for PSP and PS1. NKit keeps only the
       source CRC32, so it gives a candidate at most.

    Not used:
    - RetroAchievements' hash database needs an account key. Its
      per-system hashing rules (MIT, in rcheevos) can guide steps 1 and 3.
    - ScreenScraper needs keys; it is already tier 2.
    - Hasheous: a keyless lookup isn't confirmed (see 11).

    Small fixes found along the way:
    - a second copy of a shared audio track is marked "duplicate" by path
      order, even when it belongs to another disc;
    - files with unknown extensions are skipped silently (only counted).

## Later ideas (parked, do not build yet)

- Arcade: MAME / FBNeo DATs (libretro mirrors them); merged/split sets need
  their own matching rules.
- Online lookups for files that stay unknown: RetroAchievements hashes
  (login token), ScreenScraper (keys); Hasheous needs neither, see
  milestone 11.
- `.m3u` playlists for multi-disc games (organize would write a new file).
- Play history and favourites.

- Redump update check without re-downloading (Redump has no version index).
- The regions seed from remustwo for 1G1R (parent/clone data is in
  milestone 12).
- RAR support needs a free pure-Rust decoder; none known yet.
