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
   **Open:** scan the rest of the collection.
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
   ScreenScraper, SteamGridDB and IGDB came in milestone 9 (optional keys),
   the LaunchBox database in 11 (a download the user makes).
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
   their sheet. `--dry-run` prints the command. Milestone 14 added DOSBox
   Staging, DOSBox-X, ScummVM and MAME, and the first runs with real
   programs.
   **Open:** the console emulators above are still tried with stand-in
   scripts only.

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
   with Tauri's opener plugin, https only). Search (milestone 8) and
   Cleanup & rename (10) have since been built.
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
       *(Built with 12.4: Details' "Edition" line. Search shows the
       evidence of 12.6, region agreement among it.)*
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
    2. *Keep more of the DAT* (small) *(built 2026-10-18)*. Games keep
       `cloneof`, `region` (libretro's No-Intro mirror has it on most
       games) and Redump's `category`. Serials go in a `serials` table by
       system and title, so they outlive a DAT's re-import, with a key of
       letters and digits only (`SLUS_000.67` = `SLUS-00067`). Sources:
       - Redump DATs are now fetched as `datfile/<code>/serial,version`,
         which lists each game's serials without an account (PS1: 10,641
         of 10,914 games); same source family as before, so it replaces
         the old import. Redump leaves `&` unescaped in some serials
         (`H&D/CDCRYST/E`), so a bare `&` is escaped before parsing;
       - for cartridge systems, libretro's `metadat/serial` list
         (CC BY-SA 4.0) right after the No-Intro DATs. It is stored under
         its own title and under every DAT game with the same CRC32.

       `Library::titles_by_serial` narrows by serial: several discs or
       versions share one (Final Fantasy VII's three discs). Migration 15
       clears the DATs' stored hashes, so the next download reads them
       all again. Parent/clone: libretro's mirror and Redump carry none,
       and No-Intro's own parent/clone DATs need DAT-o-MATIC's form.
       *(Built 2026-10-18.)* Retool's clone lists (BSD-3-Clause, on
       GitHub, still updated though Retool itself isn't) fill the gap.
       - **Fetching:** `dat download` stores each system's list (`<DAT
         name> (No-Intro|Redump).json`) in `clone_groups` (migration 21).
         Compilation-only and regex entries are skipped.
       - **1G1R:** titles a list groups are one title ("Aero Fighters" =
         "Sonic Wings"). It ranks retail, then region, then the list's
         priority, then revision.
       - **On the real library:** lists for SNES, Genesis, PS1 and GBA
         (3,477 titles), and the 1G1R plan is unchanged.
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

       *(Built 2026-10-18.)* `ident.rs` holds one table of readers. Each
       checks a structure that can't occur by chance (a checksum, a magic,
       ISO 9660 boot files), so nothing is guessed from extensions:
       - GBA, NDS, GB/GBC, N64, Mega Drive, SNES;
       - PS1/PS2 `SYSTEM.CNF`, or a root file named like `SLUS_007.47`
         when `SYSTEM.CNF` lies beyond reach;
       - PSP `UMD_DATA.BIN`, Saturn, Sega CD, Dreamcast, GameCube, Wii.

       How it runs:
       - readers get the first 1 MiB of each stream as it is hashed, in
         the DAT's form (header stripped, byte order fixed), falling back
         to the raw bytes;
       - results go in `files.id_*` (migration 16, which also re-reads
         files not matched yet) and show next to each file in Details;
       - not yet read: 3DS, Switch, Xbox, GDI's high-density area.

       On the real library:
       - every cartridge and all 5 PS1 discs gave an ID;
       - each one's serial leads back to its own game. PS1 serials match
         one game exactly; 4-letter cartridge codes (`BPEE`, `ADZE`)
         narrow to 1–3 titles, matched inside the full serial
         (`AGB-BPEE-USA`) in step 6;
       - the fan translation reads `AQ3J`, Dragon Quest III's code.
    4. *Parse names* (small to medium). `shiratsu-naming` (MIT, 1.1.0)
       tokenizes No-Intro, TOSEC and GoodTools names into base title,
       region, languages, revision, dev status and flags (`[b]`, `[!]`,
       `[h]`, `[tr en]`, `[T+Eng]`). It serves 11.4 too, and reads the file
       names of unknown files.

       *(Built 2026-10-18.)* `names::parse` gives one `Name`: title,
       regions (as written and as codes), languages, version, release
       status, disc, bad / verified / alternate dump, hack, translation,
       unlicensed. The order it tries:
       - No-Intro (and Redump);
       - TOSEC, when there is a date;
       - GoodTools, only when a GoodTools-only token is present;
       - otherwise a loose reading: the title before the first bracket;
         an unknown first group as the region ("Czech"); `(Disc N)`; and
         translation or hack markers ("(English v2.0)[Addendum]",
         "[Hack by X]").

       Over the 889,831 names in all imported DATs (under 2 s), 99.9% are
       read by their own convention; the rest are BIOS short names and
       hack entries, which the loose reading handles. 1G1R now ranks
       with it: retail first, then region, then revision (now also
       "Rev A"). Details shows an "Edition" line, e.g. "USA · Rev 1" or
       "Japan · translation: English v2.0 Addendum".
    5. *A title index with vetoes* (medium), built per system from the
       DATs:
       - normalize articles, "&", accents and punctuation, turn Roman
         numerals into digits, and keep the subtitle as a field of its
         own;
       - hard tokens veto a match: numerals, years, and Jr./DX/Plus/
         Turbo/Special ("Street Fighter II" never matches "III");
       - score token-set similarity, built on `strsim` (MIT). Jaro-Winkler
         alone is unsafe ("II" vs "III" scores about 0.98).

       *(Built 2026-10-18.)* `titles.rs` keys a title: tags dropped,
       articles dropped wherever they are, accents folded, "&" as "and",
       Roman numerals II–XX as digits ("I" only at the end), and the
       subtitle as a field of its own. Hard tokens are numbers, years and
       edition words (Jr., DX, Turbo, Gold/Silver, ...).

       Scores:
       - 1.0: the same words;
       - 0.9: the same main title, one side without a subtitle;
       - 0.85: every word of one side (two at least) is in the other;
       - otherwise word overlap, below 0.85, with one-typo tolerance for
         long words;
       - 0 when the hard tokens differ, or when no ordinary word is
         shared.

       `TitleIndex` finds candidates through shared words. Search's DAT
       suggestions and Wikidata labels both use the score; a search takes
       about 0.25 s on the real library. There, the fan translation's top
       suggestion is Dragon Quest III (Japan) at 90% (the next at 37%),
       and "zelda link to the past" finds A Link to the Past at 85%.
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

       *(Built 2026-10-18.)* `suggest.rs` gathers candidates for a row
       from three places:
       - CRC32-only hits;
       - DAT games whose serial carries the file's header ID (exact, or a
         4-letter code inside the serial);
       - titles alike to the file name or the header title, on the row's
         systems (its own; else the ID's kind via the new `ident` column
         in `systems.rs`, then the extension).

       Each candidate is weighed on name, header title (compared run
       together and as a prefix, since headers squash and cut titles),
       size, CRC, serial, region (header or name) and hash. The bands:
       - *Likely: CRC match*: CRC and size agree;
       - *Likely: modified copy*: same size, and the name or serial agrees;
       - *Suggested: name match*: the name scores 0.9 or more, or the
         serial agrees with a fitting name or header title;
       - *Possible: similar name*: anything weaker that still shares a
         name or serial.

       Search's DAT results now come from it, with a band and a list of
       evidence (+/−) in the dialog; the queue (CLI and app) shows each
       unknown file's top suggestion. A `Suggester` keeps each set of
       systems' title index across rows (the queue takes about 0.6 s on
       the real library). On the real library the fan translation shows:
       *Suggested: Dragon Quest III - Soshite Densetsu e... (Japan)*, with
       + the same title (subtitle missing), + header title DRAGONQUEST3,
       + header code AQ3J in its serial SHVC-AQ3J-JPN, + region Japan,
       − 6 MiB vs 4 MiB, − no SHA1/MD5 match.
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

       *(Built 2026-10-18.)*
       - **Status:** `RowStatus::Derivative`, "translation of X" or "hack
         of X". `manual_ids` keeps `base`, `relation` and `tag`
         (migration 17). It counts as identified wherever "identified by
         you" does (cleanup, play, the queue) and is never verified.
       - **Listing:** it is listed as "<base> [Translation: tag]".
         Details shows "Translation of …"; metadata and art fall back to
         the base game's, which Enrich now fetches too (also in a
         one-title run).
       - **Suggestions** propose the relation when the file's name says
         translation or hack ("its name says translation: English v2.0
         Addendum"). Taking such a result in Search records it. The CLI
         has `search name --translation-of BASE` / `--hack-of BASE` and
         `--tag`.
       - **Patches:** a valid BPS or UPS patch beside a plain file (same
         stem) gives the source and target size and CRC32, checked by the
         patch's own CRC32. A DAT ROM with the source size and CRC32
         becomes a candidate. "Turns its ROM into this file" (target CRC
         = the file's) makes it *Likely: modified copy*, proposed as a
         hack. This is CRC-level evidence, so never verified. IPS has no
         checksums and is not read.
       - *(Built 2026-10-18, follow-up.)* `patch.rs` reads and applies
         IPS (with RLE and truncation), BPS and UPS. It finds patches
         beside a file on disk or in the same zip / 7z.
         - **Proof:** a patch applied to your hash-verified copy of the
           base, giving exactly the file's SHA1, gives the band *Patch of
           it (proven)*, still never verified.
         - **IPS:** it names no source, so it is tried on your verified
           copies of the best candidates.
       - **Not yet:** libretro's `metadat/hacks` as a source of base
         names (its entries are hash-matched DAT games already).

       On the real library, the fan translation's top suggestion now
       reads *Suggested: name match · translation of Dragon Quest III -
       Soshite Densetsu e... (Japan)*, with 5 points for (title, header
       title, header code AQ3J in SHVC-AQ3J-JPN, region, "its name says
       translation: English v2.0 Addendum") and 2 against (size, hash).
    8. *Cue sheets by content* (small). Compare a cue's tracks (file names,
       modes, indexes) with the DAT's cue instead of its bytes. A cue
       rewritten with CRLF or other spacing then shows as "cue fits", not
       as an unknown file. The library's Symphony of the Night cue is 252
       bytes against Redump's 282, with the same two tracks.

       *(Built 2026-10-18.)* A DAT keeps a cue's hash, not its text, so
       "by content" means by its tracks:
       - **Scan:** keeps the FILE names of every small text file that
         reads as a cue sheet (`files.sheet_files`).
       - **Identify:** a cue with no hash match whose FILE lines name
         exactly a game's DAT tracks, in order, all hash-matched in the
         same archive or folder, gets the new file status `sheet`. It is
         matched to the game's cue ROM with `via = 'sheet'`, which never
         counts as had or verified.
       - **Library:** the sheet joins its game's row, which shows "cue
         rewritten" instead of "cue differs" (`cue_fits`).
       - **Migration 18:** rebuilds `files` for the new status (SQLite
         can't change a CHECK) and re-reads cue sheets not matched.
       - **On the real library:** the Symphony of the Night cue is now a
         `sheet` of its game, and only the fan translation stays unknown.
       - *(Follow-up.)* GDI sheets are read too. Redump's Dreamcast DATs
         use cue sheets, so a user's GDI fits like a rewritten cue
         (migration 20).
    9. *Containers* (large, last). RVZ/WIA/WBFS/CISO/GCZ store the disc's
       6-byte ID and size (RVZ: `iso_file_size`) but no hash of the
       original ISO. ID + size gives a suggestion at once; a strict match
       needs streaming decompression inside romforge (house rule 7).
       CSO/ZSO/PBP work the same way for PSP and PS1. NKit keeps only the
       source CRC32, so it gives a candidate at most.

       *(Built 2026-10-18.)* `ident::read_container` reads just a plain
       file's header and first block (seeking; the 1 MiB prefix can't
       hold GCZ's or CSO's block tables). What it reads:
       - RVZ / WIA (Dolphin's layout: big-endian ISO size at 0x24, the
         disc's first 0x80 bytes at 0x58);
       - WBFS (the disc head after the first HD sector);
       - Wii/GC CISO (the disc from 0x8000);
       - GCZ (header with the disc size, then the first zlib block);
       - PSP/PS2 CSO (deflate) and ZSO (LZ4, `lz4_flex`, MIT): the first
         1 MiB of blocks, read as an ISO for `UMD_DATA.BIN` /
         `SYSTEM.CNF`;
       - PBP's PARAM.SFO (DISC_ID, TITLE; CATEGORY "ME" is a PS1
         classic).

       How it is used:
       - the plain image's size goes in `files.id_size` (migration 19,
         which re-reads such files);
       - suggestions compare that size with the DAT's, and a GameCube /
         Wii ID's 4-letter game code with Redump's serials
         (`DL-DOL-GALE-USA`);
       - a new band, *Likely: compressed image*, means the image size
         and serial agree. A strict match still needs the whole image
         decompressed, which romforge doesn't do;
       - *(Follow-up.)* NKit keeps the disc header, so its ID is read as
         usual. Its header also gives the original image's CRC32 at 0x208
         ([docs.fileformat.com](https://docs.fileformat.com/id/disc-and-media/nkit);
         byte order undocumented, so both are tried). A DAT ROM with that
         CRC32 and an agreeing serial is *Likely: compressed image*.
         Stored as `files.id_crc`.

    Milestone 12 is done (2026-10-18). On the real library 28 of 30
    files match by hash, the Symphony of the Night cue is a `sheet` of its
    game, and the one unknown file (a fan translation) is suggested as a
    translation of Dragon Quest III (Japan) with five points of evidence.

    Not used:
    - RetroAchievements' hash database needs an account key. Its
      per-system hashing rules (MIT, in rcheevos) can guide steps 1 and 3.
    - ScreenScraper needs keys; it is already tier 2.
    - Hasheous: a keyless lookup isn't confirmed (see 11).

    Small fixes found along the way:
    - a second copy of a shared audio track is marked "duplicate" by path
      order, even when it belongs to another disc;
    - files with unknown extensions are skipped silently (only counted).

13. **One RomForge folder** (decided 2026-10-18; *built* the same day:
    `home.rs`, `home create|show`, `import`, the app's first-run dialog and
    Add folder → import plan). Everything romforge keeps
    lives in one portable folder, `RomForge/`, inside a parent folder the
    owner picks on first run (home suggested; Settings can move it, and
    the folder is always named RomForge). Moving or backing up that folder
    takes the whole library with it:

    ```
    RomForge/
      Games/<System>/<Title>.zip       cartridges: ROM at the top level,
                                       art and <Title>.romforge.md in romforge/
      Games/<System>/<Title>.chd       discs, with a sidecar folder <Title>/
      Media/<System>/<kind>/<Title>.*  the art (today ~/.cache/romforge/art)
      Databases/                       OpenVGDB, LaunchBox (downloaded by you)
      romforge.db                      today ~/.local/share/romforge
      .romforge-staging/  .romforge-trash/   same drive; undo
    ```

    - *Import* replaces Organize's destination and Cleanup's Library
      folder. Adding a folder scans it (read-only, rule 6 unchanged), then
      shows the import plan: which games go where in `Games/`, and what
      stays (unidentified files, partial games, conflicts). One click
      moves them, repacked as Cleanup does; journaled; "Undo last import".
    - Originals are not kept by default; they go to `RomForge/.trash/`
      until undone or emptied (the "Keep originals" setting still exists).
    - Only verified games and games you named move. Unknown files stay
      where they are until Search identifies them; then they import too.
    - First run (or an existing setup): create `RomForge/`, then move the
      database, art cache and downloaded databases into it, by the same
      journaled, verified moves. The old locations are left empty.
    - The scanned folders stay in the list, so Refresh finds new files
      dropped into them and offers them for import.
    - `RomForge/Games` itself is scanned like any folder; files put there
      by hand are identified, and renamed only by an import plan.
    - The CLI and app find the folder through `~/.config/romforge/location`
      (or `ROMFORGE_HOME`); `--db` / `ROMFORGE_DB` still override.
    - Not yet run on the real collection.

    Decided 2026-10-18 for what was left open (*built* the same day; not
    yet run on the real collection):
    1. *Copies.* A verified copy outside RomForge of a game already in
       `Games/` is trashed with the import as a copy (undo restores it),
       unless Keep originals is on or its file holds other games.
    2. *Moving the folder.* The database records the folder it was last
       opened in; opened somewhere else (moved by hand), it rewrites its
       stored paths (files, scan roots, art, journals) to the new place.
       Settings gets "Move RomForge…": pick a new parent, the folder moves
       (verified, never overwriting), the pointer file follows.
    3. *Versions grouped, not filtered* (replaces 1G1R). Every version
       imports and is kept: regions, revisions, translations, hacks. The
       library list shows one row per game, its main version chosen by
       a region order set in Settings (e.g. USA, Europe, Japan); the row
       expands to every version you have. Groups come from the clone
       lists (milestone 12, `clone_groups`) and DAT `cloneof`, then the
       base title; translations and hacks join their base game's group.
    4. *Organize* stays for setups without a RomForge folder; with one,
       its screen and command point to Import.

14. **DOS, ScummVM and Japanese computers** (decided 2026-10-18; *built*
    the same day: `scummvm.rs`, folder rows in `view.rs`, folder imports
    in `cleanup.rs`, `scummvm read|import`; not yet run with real games or
    emulators). Researched: PC-88 and PC-98 are already systems with TOSEC
    DATs; `PC` has TOSEC and Redump but nothing to run it; libretro's
    `dat/DOS.dat` (24,631 games) names each game by one `.exe`, size and
    CRC only; `dat/ScummVM.dat` only maps ScummVM game ids to titles.
    1. *Folder games.* DOS and ScummVM games are folders of many files
       (`.exe`, `.ovl`, `MONKEY.000`, ...), which the scan skips today
       (it hashes by extension). A folder whose files make up a known
       game is one game. Import packs a DOS game into
       `Games/DOS/<Title>.zip` (DOSBox Pure runs it as is) and keeps a
       ScummVM game as the folder `Games/ScummVM/<Title>/`, with a
       `<id>.scummvm` file for RetroArch's core. Rule 6 is unchanged:
       scanning reads, only import moves.
    2. *DOS identification stays strict.* TOSEC disk images and Redump CDs
       verify (size + SHA1/MD5). A `DOS.dat` hit (size + CRC of the main
       `.exe`) is only a likely title, shown as a suggestion; confirming
       it makes the game "identified by you", never verified.
    3. *ScummVM identification from your own ScummVM.* romforge runs the
       installed ScummVM once with `--dump-all-detection-entries`, which
       writes its detection list as a DAT (files by name, size and MD5 of
       their first N bytes, usually 5000), stores it in `Databases/`, and
       matches folders itself, as ScummVM does: every listed file present
       with that size and partial MD5. That needs a partial-MD5 hash in the
       scan, kept apart from full hashes. ScummVM's detection data is GPL:
       generated on the user's machine, never shipped (rule 10's terms).
       Unverified: whether the SCUMM engine's own games (Monkey Island,
       ...) are in that dump; if not, they are named by hand from
       `ScummVM.dat`'s list.
    4. *Launch.* DOS: the `dosbox_pure` core (zip or image as is), then
       DOSBox Staging and DOSBox-X (on `PATH` or Flatpak) with the folder
       or image mounted. ScummVM: the `scummvm` core with the `.scummvm`
       file, then standalone ScummVM (`--path=<folder> <id>`). PC-88 gets
       `quasi88`; PC-98 more disk formats (`hdm`, `nhd`, `fdd`, `thd`,
       ...).
    5. *Multi-disk playlists* (from Later ideas). For a set of disks
       ("Disk 1 of 4"), import writes a `<Title>.m3u` next to them, so
       the emulator can swap disks; launch starts the playlist.
    6. *More Japanese computers.* Launchers for Sharp X1 (`x1` core),
       FM-7 and FM Towns where a free emulator exists (to check which).
    As built: a new `DOS` system takes TOSEC's IBM PC DATs and `DOS.dat`
    from `PC` (which keeps Redump's CDs and Windows); `ScummVM` and `FM-7`
    are new systems. A ScummVM folder is "identified by ScummVM's list"
    (its own status), worked out again after every scan. A program loose
    in a scanned folder is not taken as a folder game. Playlists list the
    zips (or CHDs) as imported.
    **Open:** naming a ScummVM folder by hand is not built (real-program
    runs: below).

    Checked 2026-10-18 (the cores' and ScummVM's source, the TOSEC pack,
    libretro's servers, Wikidata, the local LaunchBox and OpenVGDB copies):
    - *Floppy cores and playlists.* None reads a zip listed in a playlist:
      each opens playlist entries itself as plain files. `quasi88` (PC-88)
      reads `.m3u`, but takes content without a full path, so a zipped
      game likely fails too. `np2kai` (PC-98) dropped `.m3u` under
      libretro; it reads a `.cmd` file (a command line of disk images,
      which it puts in its disk-swap list) and gets single zips unpacked
      by RetroArch. `x1` lists `zip` as its own format but has no zip
      code, so RetroArch hands it the zip unopened: zipped X1 games fail;
      no playlist, disks swap from RetroArch's menu only. So the playlists
      import writes today (zips) don't work for these cores.
    - *ScummVM's dump leaves out SCUMM, SCI, Sky and Glk.* Their
      `dumpDetectionEntries` is empty or `#if 0`: no LucasArts or
      Humongous game, no Sierra SCI game (King's Quest IV on, Space Quest,
      ...), not Beneath a Steel Sky, no interactive fiction. The rest
      (AGI, AGOS, Kyra, Lure, Queen, ...) are in it.
    - *TOSEC.* "Fujitsu FM-7" is right (10 DATs: 111 D77 and 169 T77
      games). Also in the pack and now added: "Fujitsu FM77-AV" (to FM-7)
      and "IBM PCjr" (to DOS). Not added: NEC PC-8001, PC-6001, PC-88VA,
      Sharp MZ (no system or core here yet).
    - *Thumbnails (libretro).* DOS 4,470 box arts, named by `DOS.dat`
      names (what a confirmed DOS game is called); PC-98 726 and PC-88 322,
      named by TOSEC names; X68000 2,140; X1 only 13; FM-7 and FM Towns
      none. PC-88's folder is "NEC - PC-8001 - PC-8801" (fixed in the
      table). ScummVM 728, named by `ScummVM.dat` names, which romforge's
      ScummVM titles ("Beneath a Steel Sky (CD) (pc) (en)") don't match.
    - *Metadata.* libretro has no `metadat/<field>/` files for DOS,
      ScummVM or these computers. But `dat/DOS.dat` carries genre, year
      and developer per game, and `dat/ScummVM.dat` description, year,
      developer and publisher per ScummVM game id; romforge reads neither
      as metadata yet.
    - *Wikidata* (video games per platform item, now in the table): DOS
      2,414 (Q170434) + 302 (MS-DOS, Q47604); PC-98 223; PC-88 131
      (Q1338888); FM Towns 93; X68000 84; X1 67; FM-7 57. ScummVM isn't a
      platform there.
    - *LaunchBox* (the downloaded copy): MS-DOS 7,199; PC-9801 2,388;
      X68000 1,114; PC-8801 1,080; FM Towns Marty 626; X1 334. It has no
      FM-7 and no ScummVM platform (ScummVM games are under MS-DOS);
      FM-7's made-up LaunchBox name is gone from the table.
    - *OpenVGDB* has none of these systems. ScreenScraper and IGDB ids for
      them are in the table from memory, unverified like the others.
    - *Versions.* TOSEC lists each disk as its own game, so the disks of a
      set may group as "versions" of disk 1 in the library; not checked.

    Decided 2026-10-18 from that (*built* the same day; not yet run with
    real games or emulators):
    1. *Floppy sets loose.* For PC-88, PC-98, X1 and FM-7, import puts a
       multi-disk set's images unzipped in `Games/<System>/<Set>/`, with
       the playlist its core reads (`.m3u` for quasi88, `.cmd` for
       np2kai); X1 has none (RetroArch's disk menu). Single disks stay one
       zip each. An exception to milestone 10's one archive per game, for
       these systems only. Still journaled and verified; undo as before.
    2. *ScummVM's own --detect for what its list misses.* A folder the list
       didn't name (SCUMM, SCI, Sky, Glk) is shown to the installed
       ScummVM with `--detect --path=<folder>`; the game id it prints
       names the folder ("identified by ScummVM", never verified). An
       exception to rule 7, for folders the list missed only.
    3. *libretro's DOS.dat and ScummVM.dat as names and metadata.* A
       ScummVM folder is titled by `ScummVM.dat`'s name for its game id
       (so libretro's box arts match), versions told apart as now; DOS
       and ScummVM games get description, genre, year, developer and
       publisher from those files. Never used to verify (ScummVM.dat's
       own hashes are of `.scummvm` text files).
    As built: `loose_sets` in `systems.rs` (PC-88 `m3u`, PC-98 `cmd`, X1
    and FM-7 none); a set's folder is journaled (`setdir`) and removed by
    undo once empty. `scummvm read` also runs `--detect --recursive` once
    per scanned folder (`scummvm_ids`, by path and file fingerprint);
    `ScummVM.dat` is downloaded to `Databases/scummvm-titles.dat` (the
    `titles` column: read for names and metadata, never imported); enrich
    reads `dat/<name>.dat` for the `dats` and `titles` columns, filling
    only empty fields. Details files (`*.romforge.md`) are no longer
    scanned as Mega Drive ROMs.
    Run with the real programs 2026-10-18 (installed: RetroArch, its
    scummvm core, ScummVM 2026.3.0 and MAME from Manjaro; np2kai, quasi88,
    x1 and dosbox_pure from libretro's buildbot into
    `~/.config/retroarch/cores`; DOSBox Staging and DOSBox-X from
    Flathub), on ScummVM's freeware Flight of the Amazon Queen, Lure of
    the Temptress and Beneath a Steel Sky, in scratch folders:
    - The real dump has 15,598 entries; 12,746 are usable (the rest need
      Mac forks or name wildcards). It escapes quotes (`5.25\" Booter`)
      and has files with no MD5 (`md5-0  )`): both broke the parser, now
      handled. Queen and Lure were named by the list, Steel Sky by
      `--detect` (which lists a folder once per variant; the first is
      kept). Import, refresh and both ScummVM and RetroArch's core
      started Lure.
    - Fixed: a ScummVM folder holding a DOS program (Lure.exe) was started
      from the program; the CLI's `launch <title>` found no folder games.
    - The DOSBox Flatpaks see only home ("Image file not found" for a
      folder elsewhere): every Flatpak emulator now gets the game's folder
      for that run (`flatpak run --filesystem=<folder>`). DOSBox Pure ran
      a zipped DOS folder as is.
    **Open:** np2kai, quasi88, x1 and MAME's FM-7 / FM Towns need the
    machines' own BIOS files (not shipped; the user supplies them), so no
    floppy game was started; single zipped PC-88 and X1 games likely don't
    start (see above) and stay zipped as decided. Parsing the 4 MB list
    takes about 2 s (debug build) on every scan.

15. **Packages** (decided 2026-10-18; *built* the same day:
    `tauri.conf.json` bundles, `.github/workflows/release.yml`; no release
    tagged yet). romforge starts host
    programs (RetroArch, ScummVM, MAME, `flatpak run`), reads and moves
    files on any drive, and needs WebKitGTK (134 MiB installed on Arch).
    So:
    - *Native packages first.* An AUR package (PKGBUILD in the AUR's own
      git, not here; rule 5) and Tauri's `.deb` and `.rpm`: the system's
      WebKitGTK, nothing sandboxed, the app and `romforge-cli` together,
      and the emulators as optional dependencies (RetroArch and its cores,
      ScummVM, MAME, DOSBox).
    - *AppImage* stays as the portable fallback (it bundles WebKit and
      GStreamer: much bigger, and needs FUSE).
    - *No Flatpak*: its sandbox can start host emulators only through
      `flatpak-spawn --host` (in effect an escape) and needs the whole
      filesystem.
    - *Releases from CI.* Pushing a tag `vX.Y.Z` builds every package and
      attaches them, with SHA-256 checksums, to a GitHub release. CI today
      builds the app without bundling.
    As built and tried locally: `.deb` and `.rpm` 14.9 MiB each (app,
    `romforge-cli`, desktop entry, icons, licence; they depend on the
    system's WebKitGTK and GTK, and recommend only RetroArch and ScummVM,
    since apt and dnf install recommended packages by default and Tauri
    has no "suggests"); AppImage 105 MiB (app only). The release job
    writes the AUR's `romforge-bin` PKGBUILD (repackaging the `.deb`, all
    emulators as optdepends); `makepkg` builds it and `namcap` is clean.
    The packaged app and the AppImage started on a scratch library.
    **Open:** the first tagged release (the workflow has not run on
    GitHub yet); pushing `romforge-bin` to the AUR (the owner's account);
    Debian's and Fedora's package names for the recommended emulators are
    unchecked.

16. **Trash, two matching fixes, deterministic zips** (decided
    2026-10-18; *built* the same day, tested on scratch folders).
    1. *Empty the trash.* Import and cleanup keep originals in
       `.romforge-trash/<batch>/` for undo, and nothing ever removes them
       (the app even says "until you undo or empty it"). An "Empty trash"
       action in the app and `cleanup empty-trash` in the CLI: every
       batch, or those older than N days; it shows what goes and how much
       space it frees, asks first, deletes for good, and marks those
       batches as no longer undoable (their journal says so).
    2. *Shared audio tracks.* A second copy of a track two discs share is
       marked "duplicate" by path order even when it belongs to the other
       disc; the copy beside each disc's other tracks should count for
       that disc.
    3. *Unknown extensions shown.* Files the scan skips for their
       extension are only counted; list them (path and extension) in the
       scan report and the app, so a format romforge doesn't know is
       visible.
    4. *Deterministic zips.* Import writes zips with the current time, so
       the same game zips to different bytes each time. Fixed timestamps,
       members in a fixed order, one compression setting (TorrentZip's
       rules, as far as the `zip` crate allows): the same game always
       gives the same file, so re-checks, syncs and backups are exact.
    As built: `cleanup empty-trash [--older-than N] [--yes]` and "Empty
    trash…" in the dialog; a batch is marked emptied (migration 28) before
    anything is deleted, and undo refuses it. ROM ownership is per ROM and
    recorded (`matches.owns`, migration 29, seeded from the old rule until
    the next scan): a game's row lists the copies that own its ROMs and
    their duplicates, and import prefers the owning copy. Scan reports
    carry `skipped_kinds`. Zips: fixed date, 0644, level 6, members by
    lower-case name, zip64 only when needed; a folder game from a 7z is
    unpacked to staging first, and one that changed since the plan is
    refused (it goes to the trash whole).

17. **Arcade: MAME and FBNeo** (decided 2026-10-18; *built* the same day,
    tested on scratch sets; not yet run on real arcade sets). Checked:
    libretro mirrors per-ROM MAME XMLs with SHA1 (2003, 2003-Plus, 2010,
    2015, 2016) and DATs of whole TorrentZipped sets (one hash per zip,
    matching only byte-identical zips); FBNeo's own DATs list CRC32 only;
    an installed MAME writes its own (`-listxml`, SHA1 per ROM and disk).
    1. *Three arcade systems, one per emulator family*, since a set only
       runs in the version its DAT belongs to: Arcade (current MAME: the
       installed MAME's `-listxml`, made on the owner's machine like
       ScummVM's list, never shipped), MAME 2003-Plus (libretro's per-ROM
       XML, the `mame2003_plus` core) and FBNeo (FBNeo's DAT, the `fbneo`
       core). Each has its own `Games/<System>/` folder.
    2. *FBNeo verified through MAME.* FBNeo's DAT has only CRC32 (never a
       match, rule 8). An FBNeo ROM counts when the file's size and CRC32
       fit it and the file's SHA1 fits the same ROM (size and CRC32) in a
       MAME DAT you have; else it stays a suggestion.
    3. *Sets move as they are.* Zips keep their set names (emulators find
       them by name) and their merged / split / non-merged form; a clone
       goes with its parent, BIOS sets go along; nothing is repacked.
    4. *Disks by the CHD's own SHA1.* MAME's DAT gives each disk's SHA1,
       which a CHD keeps in its header: read without decompressing; the
       set's CHD folder moves with it.
    The scan hashes every member of a zip or 7z named after an arcade set
    (arcade ROMs have no telling extensions).
    As built: `arcade` and `urls` columns in `systems.rs` (Arcade, MAME
    2003-Plus, FBNeo); the DAT parser reads MAME's `<mame>` / `<machine>`
    (devices skipped), `<disk>` (a ROM with `disk`, migration 30) and
    `optional`; `files.chd_sha1` from each CHD's header; via `confirmed`
    for FBNeo (in `strong!()`); `Format::Set` copies whole files; rows
    carry `label` (the description); `{dir}` and `{set}` in emulator
    arguments; `romforge-cli dat mame` and "Read MAME's list". The real
    MAME 0.289 list imports in 10 s (43,050 games, 370,102 ROMs, 1,402
    disks; 70 MB of database); MAME 2003-Plus 4,874 games, FBNeo 8,451.
    **Open:** a set valid in two systems' DATs goes to the first one
    Import plans (the other then finds it there); RetroArch can't start a
    clone of a merged set (it has no zip of its own; MAME can); arcade
    metadata (genre, year) isn't fetched; real CHDs are hashed in full
    as well as read for their SHA1 (slow for big hard disks).

## Launcher (milestone 18; researched 2026-10-18, not built)

Folded in from the separate launcher plan the owner kept apart until it was
decided.

### Goal

romforge becomes a self-contained launcher, like RetroDECK or LaunchBox /
Big Box: pick a game, it plays, with no emulator UI, setup or desktop in
between. Emulators run "headless" where possible, meaning romforge owns
their window, config, folders, hotkeys and exit.

### Where we start

Milestone 5 already launches. `launch.rs` finds RetroArch (on `PATH` or
the Flatpak `org.libretro.RetroArch`) and its cores, and 16 standalone
emulators (`KNOWN_EMULATORS`; milestone 14 added DOSBox Staging, DOSBox-X,
ScummVM and MAME), using the `cores` and `emulators` columns of
`systems.rs`. It builds a command line and spawns it; a Flatpak emulator is
given the game's folder for that run. What is missing:
- romforge does not own any emulator's config, so each one opens with its
  own UI, windowed, with its own save folders and hotkeys;
- no way to install an emulator or core from romforge;
- no BIOS check, no unified saves, no controller-driven exit;
- only ScummVM, RetroArch's ScummVM core and DOSBox Pure have been run
  for real (milestone 14); the console emulators only with stand-in
  scripts.

### What "headless" can mean

| Approach | Fits | Cost |
|---|---|---|
| **A. Drive external emulators headlessly**: RetroArch and standalone emulators started fullscreen, with configs and folders romforge writes | every system, including 3D (GameCube, PS2, PS3, Switch...) | small; no linking |
| **B. Embed libretro cores**: romforge loads `*_libretro.so` itself and draws the frames | software-rendered cores (8/16-bit, handhelds, PS1 software, arcade) | large; audio/video sync, input, states |
| **C. Embed HW-rendered cores** (Dolphin, LRPS2, Flycast, Mupen64Plus-Next, Beetle PSX HW) | 3D systems | very large: GL/Vulkan context negotiation (`SET_HW_RENDER`, `libretro_vulkan.h`) |
| **D. Host standalone emulators inside romforge's window** (a nested Wayland compositor) | systems with no libretro core: PS3, Wii U, Xbox, Vita, Switch, Xbox 360 | very large; no launcher found that does it |

No mainstream launcher embeds cores. ES-DE, Playnite and LaunchBox all start
RetroArch or standalone emulators; RetroDECK bundles them in one Flatpak and
pre-writes their configs (romforge itself is not a Flatpak: milestone 15).

**Decided 2026-10-18:** romforge first *feels* like one app, the RetroDECK /
Big Box way (A, stages 1 and 2): it hands the screen to a fullscreen
emulator behind a splash and takes it back on one controller combo. *Truly
one window for everything* (B, C and D) is a planned future stage (stage 3),
built only after stages 1 and 2 are solid. Stage 1 stays as its fallback.

### Stage 1: own the emulators (approach A)

#### 1.1 One emulator table

Extend `KNOWN_EMULATORS` (one table, rule 4) after ES-DE's model:
- find rules in order: `PATH`, Flatpak (`/var/lib/flatpak/exports/bin`,
  `~/.local/share/flatpak/exports/bin`), AppImage globs
  (`~/Applications/<name>*.AppImage`), `RomForge/Emulators/`, a user path;
- a command template with variables (`{rom}`, `{core}`, `{config}`,
  `{saves}`, `{states}`, `{bios}`);
- per emulator: fullscreen/no-UI flags, how to point it at a config dir,
  whether it reads archives, how to quit cleanly.

ES-DE's `es_find_rules.xml` and `es_systems.xml` are MIT and can be adapted
with attribution. Playnite's `emulator.yaml` definitions are MIT too, but
Windows-oriented.

*(Built 2026-10-19, first part.)* `launch/table.rs` is the one table,
RetroArch included (`RETROARCH`, `known(name)`). Each entry has its
game arguments (`{rom}`, `{dir}`, `{set}`, `{core}`), `fullscreen`
flags put before them on every start, `config` flags for a romforge
config folder (`{config}`, used from 1.3), and an AppImage prefix.
Flags are filled in only once checked; so far on the dev machine's
installs: RetroArch 1.22.2 (`-f`, `--config`), ScummVM 2026.3.0 (`-f`,
`--config=`), MAME 0.289 (`-nowindow`, `-inipath`), DOSBox Staging 0.83
(`--fullscreen`, `--noprimaryconf --conf`), DOSBox-X 2026.10.01
(`-fullscreen`, `-conf`). Find order: the program set in Emulator
Settings (milestone 21), `PATH`,
Flatpak, then `~/Applications` and `RomForge/Emulators/` (a program by
name or `<name>*.AppImage`, any case, newest name first);
`RomForge/Emulators/cores/` is searched before the system's cores.
`Library::env()` builds this for the CLI and the app.
The console emulators' flags followed the same day, read from each
one's source at its release (see 1.3). Not yet: the per-emulator quit
method (with 1.6), and the path setting in the app (the Emulators page,
milestone 21).

#### 1.2 RomForge folder grows

```
RomForge/
  Emulators/            cores and AppImages romforge downloaded for you
  Emulators/config/     romforge-owned configs, one folder per emulator
  Bios/                 RetroArch's system layout
  Saves/<System>/       SRAM / memory cards
  States/<System>/      save states
  Screenshots/<System>/
```

Saves in one tree means backup or sync is a copy of one folder (rclone or
Syncthing, outside romforge).

*(Built 2026-10-19.)* `Emulators/` (with `config/<emulator>/`), `Bios/`,
`Saves/`, `States/` and `Screenshots/` are in `layout.rs` and `Home`;
`home create` makes them, and a launch makes what it needs in an
existing folder. Per-system folders are named by the system's key, as
`Games/` is (`Saves/Game Boy Advance/`).

#### 1.3 Owned configs, written as patches

*Changed 2026-10-19 (owner's choice):* romforge runs every emulator from
its own copy of the config in `RomForge/Emulators/config/<emulator>/`
and controls that copy completely; the user changes any setting from
inside romforge (milestone 21). The emulator's own config (the one it
uses when started outside romforge) is never read or written. romforge
records what it wrote, so drift after an emulator update can be seen and
re-applied. Config schemas are versioned per emulator, with migrations in
the core (RetroDECK's `post_update` idea). "Reset to romforge's
settings" per emulator.

- **RetroArch:** `retroarch -f -L <core> --config <RomForge>/Emulators/config/retroarch/retroarch.cfg --appendconfig <per-game.cfg> <rom>`.
  The config sets `savefile_directory`, `savestate_directory`,
  `system_directory`, `screenshot_directory` into RomForge,
  `config_save_on_exit = "false"`, `pause_nonactive = "false"`, the menu
  toggle unbound, and `network_cmd_enable = "true"` (UDP 55355). Then
  romforge can send `PAUSE_TOGGLE`, `SAVE_STATE`, `LOAD_STATE`,
  `SCREENSHOT`, `RESET`, `QUIT`, and poll `GET_STATUS`.
- **Standalone** (read from each one's source at the release named,
  2026-10-19; fullscreen flags are in `launch/table.rs`). Data folders:

  | Emulator | Its data folder |
  |---|---|
  | DuckStation (rolling, 2026-10-13) | `portable.txt` next to the program or AppImage; no flag |
  | PCSX2 2.8.2 | `-datapath <dir>` (data in `<dir>/PCSX2`), or `portable.txt` |
  | Dolphin 2609a | `-u <dir>`; ignored under Flatpak (always XDG) |
  | PPSSPP 1.20.4 | none on Linux (`$XDG_CONFIG_HOME/ppsspp`) |
  | Flycast 2.7 | `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME`, `FLYCAST_BIOS_PATH` |
  | mGBA 0.10.5 | `portable.ini` next to the program |
  | melonDS 1.1 | a `portable/` folder next to the program |
  | xemu 0.8.137 | `-config_path <file.toml>` (the TOML only); `xemu.toml` next to the program |
  | RPCS3 0.0.43 | a `portable/` folder next to the program; `--config <file>` for a boot |
  | Mednafen 1.32.1 | `MEDNAFEN_HOME` |
  | Cemu 2.6 | a `portable/` folder next to the program or AppImage |

  So for most, 1.3 sets environment variables (`XDG_CONFIG_HOME`,
  `XDG_DATA_HOME`, `MEDNAFEN_HOME`) for the child process, or keeps a
  portable copy of the program in `RomForge/Emulators/`. Clean quit:
  DuckStation, PCSX2, Dolphin and Mednafen shut down cleanly on
  SIGINT/SIGTERM; PPSSPP quits on Esc with `--escape-exit`; the rest are
  unchecked (1.6).

  Flatpak emulators keep config under `~/.var/app/<id>/`; they get
  `--filesystem=<RomForge>` and config written there, or a portable dir
  where the emulator supports one.

*(Built 2026-10-19 for RetroArch.)* `launch/owned.rs`: with a RomForge
folder, RetroArch starts as `retroarch -f --config
<RomForge>/Emulators/config/retroarch/retroarch.cfg --appendconfig
…/systems/<system>.cfg -L <core> <game>`. romforge sets, on every start,
in `retroarch.cfg`: `system_directory` (`Bios/RetroArch`),
`config_save_on_exit` false, `pause_nonactive` false, the
`*_in_content_dir` and `sort_*` keys false; in the system's file:
`savefile_directory`, `savestate_directory` and `screenshot_directory`
(`Saves/`, `States/`, `Screenshots/` per system; RetroArch has one save
folder, so each system gets an append file). Every other key in those
files is kept, in order. Keys checked in RetroArch 1.22.2's own config;
`--save`/`--savestate` are deprecated there, hence `--appendconfig`.
The files are written when the game starts (`spawn`), never by a plan or
`--dry-run`, which prints them. A Flatpak RetroArch is also given the
RomForge folder for that run. Without a RomForge folder RetroArch starts
as it is set up.
Not yet:
- `network_cmd_enable` (an open UDP port) waits for 1.6, which uses it;
- the menu toggle stays bound until 1.6 gives a controller exit;
- a new copy starts from RetroArch's built-in defaults: whether
  controller autoconfig and assets still resolve is to check on a real
  start (else seed those paths, or import the user's config once);
- the other emulators' configs: all ten are done (milestone 20, step 4;
  RPCS3, Cemu and xemu with their game data pointed at where it is).

*(Built 2026-10-19, RetroArch's settings.)* Emulator Settings rows with
config keys (`emusettings/table.rs`, `keys`) are written into romforge's
`retroarch.cfg` at start: video driver (RetroArch's tab), and as global
settings VSync, smooth picture, integer scaling, rewind, run-ahead,
"resume where you left off" (save and load a state) and show FPS. A
setting not set anywhere is taken out of that copy, so RetroArch's own
default applies; a key set by hand there for one of these settings is
therefore replaced. Keys checked in RetroArch 1.22.2's config.

#### 1.4 Get emulators from inside romforge

Like rule 10's "the user downloads it": never shipped, fetched on request.
- **Cores** from the libretro buildbot
  (`buildbot.libretro.com/nightly/linux/x86_64/latest/`, no published terms
  found; RetroArch's own updater does the same), into
  `RomForge/Emulators/cores/`. Read each core's `.info` (licence,
  extensions, firmware). Show the licence before download and warn on
  non-commercial ones (Genesis Plus GX, the Snes9x family, PicoDrive, the
  MAME 2000–2014 family, FB Alpha, fMSX, Opera; FinalBurn Neo
  *(unverified)*).
- **RetroArch and standalone emulators:** offer the Flathub install (user
  scope) or the project's AppImage, then detect it. romforge itself never
  bundles a GPL emulator, so its MIT licence and rule 9 stay intact.

*(Built 2026-10-19.)* `obtain.rs`:
- **Cores:** Emulator Settings → RetroArch → Cores, and `emulator cores
  [--refresh]` / `get-core NAME [--yes]` / `remove-core NAME`. The
  buildbot's `.index-extended` (date and the CRC32 of the core inside
  each zip, checked 2026-10-19) and libretro's `info.zip` are kept in
  `Databases/`; the cores romforge's platforms use are listed with
  licence, where they're installed and whether the buildbot has a newer
  build. A download is checked against that CRC32 and written to
  `Emulators/cores/` (looked in first, 1.1), recorded in
  `romforge-cores.json`; only those can be updated or removed. A
  non-commercial core asks first (the app; `--yes` on the CLI). Tried on
  a scratch folder with Gambatte.
- **Emulators:** "Install from Flathub" for each emulator not installed
  (`emulator install NAME`): `flatpak remote-add --user
  --if-not-exists flathub …` then `flatpak install --user
  --noninteractive`. Every Flatpak id in the table was checked on Flathub
  (`api/v2/appstream`); Mednafen isn't on Flathub (its id was wrong and
  is gone). Not run for real here (it installs software).
- **Not yet:** AppImages from each project's releases; uninstalling.

#### 1.5 BIOS check

*Replaced 2026-10-19 by milestone 20* (the BIOS page): RetroBIOS's
metadata covers `System.dat`'s list (its RetroArch platform is built from
it) plus every standalone emulator, and files go to `RomForge/Bios/`.
RetroDECK's `bios.json` is GPL-3.0: not used.

#### 1.6 Seamless start and exit

- A borderless fullscreen splash (box art, title) from Play until the
  emulator's window maps; borderless over exclusive fullscreen. Under
  gamescope, just start the child.
- Graceful exit: the emulator's own quit (RetroArch `QUIT`, a hotkey
  chord), then SIGTERM, then kill after a timeout. Never kill first, or
  the save is lost.
- Play time and last played recorded from the process lifetime (the
  parked "play history" idea).

*(Built 2026-10-19.)* `launch/session.rs`, `launch/plays.rs`; app
`play.rs`, `NowPlaying.svelte`:
- **Start:** Play starts a tracked session (one game at a time). The
  window goes fullscreen behind a splash (`Splash.svelte`, owner's
  design): the ROMFORGE wordmark at the top, the game's box in the middle
  turning slowly (a 3D box render sways ±28°, since a full turn would
  show the picture from behind; flat box art is wrapped onto a CSS box
  with a spine that turns all the way round), and the platform's logo at
  the bottom, large (9rem, at most 16% of the screen's height) and on its
  own transparency (a faint light edge in
  dark mode so a dark logo still reads) (Wikidata's, fetched once and cached, whatever
  the sidebar shows; the platform's name until it arrives). No motion
  when the system asks for less. It stays until the emulator's window
  takes the focus, with a 15-second fallback. Not a separate borderless window, and focus
  is the sign the emulator is up (no window-mapping check on Wayland).
- **Running:** a bar under the toolbar says what's playing, for how long,
  with Stop; Play is off meanwhile.
- **Stop:** SIGTERM first (RetroArch 1.22.2's first SIGTERM is a clean
  quit, read in `platform_unix.c`; DuckStation, PCSX2, Dolphin and
  Mednafen likewise), a kill only after 10 seconds. The process is polled,
  never reaped early, so the signal can't reach a reused pid. Not yet
  checked: a Flatpak emulator (the signal reaches `flatpak run`).
- **Play time:** each run of 10 seconds or more goes into
  `play_sessions` (migration 38) by system key and title; Details shows
  "Played 3.2 h · 5 times · last 19 Oct 2026". CLI: `launch --wait`
  records it too.
- **Not yet:** the emulators' own quit commands (RetroArch's UDP `QUIT`,
  hotkey chords) and a controller combo to quit, with stage 2.

### Stage 2: controllers and a fullscreen mode (researched and decided 2026-10-20)

**Goal (owner's):** a fully native fullscreen experience like RetroDECK's
and EmuDeck's, with far more options and customization, and custom
themes. Researched online the same day (sources below); decided:

- **romforge's own fullscreen mode, in the same app**, not ES-DE or
  Pegasus in front of it. RetroDECK and EmuDeck are configurators around
  someone else's frontend: their UI is themable only as far as ES-DE or
  Pegasus allow, and their hotkeys are only as unified as each emulator
  lets them (RetroDECK lists Cemu, xemu and Vita3k as having little or
  none). romforge owns the launcher, every emulator's config, the BIOS
  setup and play time, so it can go further. The webview is fine for a
  10-foot UI; it is unfit only for reading pads (below).
- **Pads from Rust** with `gilrs` (Apache-2.0/MIT; evdev on Linux, SDL's
  GameControllerDB, hotplug, rumble), sent to the UI as events, never
  WebKitGTK's Gamepad API: WebKit gives pad input only to the focused,
  visible window, so romforge would go deaf whenever an emulator has the
  focus (and a pad appears only after a button press). Steam Input adds a
  virtual pad (28de:11ff) beside the physical one; gilrs doesn't dedupe,
  so romforge does: honour `SDL_GAMECONTROLLER_IGNORE_DEVICES` (what
  Steam sets for its games) and, when a Steam pad is there, use only
  Steam's. `tauri-plugin-gamepad` is out (non-standard licence, rule 9).
- **Navigation:** Norigin spatial navigation core (MIT) in the
  fullscreen mode, `simple-keyboard` (MIT) as the on-screen keyboard;
  not `js-spatial-navigation` (MPL-2.0, stale).
- **One hotkey table** (`hotkeys.rs`: a hotkey button plus a chord →
  an action), written into each emulator's config romforge owns:
  RetroArch's pad hotkeys and its command port (UDP 55355, no
  authentication: bound to localhost) for QUIT / SAVE_STATE / MENU;
  PCSX2's and DuckStation's native chords (`A & B`) and their PINE socket
  for saving state; the native pad hotkeys of Dolphin, PPSSPP, melonDS,
  mGBA and Flycast. Where an emulator has none (xemu, Cemu: to check),
  romforge's own watcher still does **quit**, since it owns the process.
  Keyboard injection through `/dev/uinput` only if ever needed, off by
  default, and only where it is already allowed (Steam's udev rules); no
  portal.
- **Quit** (Hotkey+Start): the emulator's own quit command first, then
  SIGTERM (1.6), then a kill; romforge back in front. **Pause:** each
  emulator's own pause menu (RetroArch's menu, PCSX2's / DuckStation's
  fullscreen menu). A romforge menu drawn over the game is parked as an
  experiment: no overlay layer on GNOME, KWin refuses focus stealing on
  Wayland, gamescope's external-overlay hook is undocumented.
- **Steam Deck:** romforge runs as the non-Steam game and starts
  emulators as its children (launchers that hand off and exit lose their
  windows under gamescope); a `--fullscreen` start option; later a
  `shortcuts.vdf` writer and Steam Input templates.
- **Themes are data, never code.** A theme is a manifest (built-in
  layouts to choose from: carousel, grid, list, game page; options it
  offers; colour schemes; aspect ratios, vertical ones too; font sizes;
  fallback variants when a game has no video or art) plus CSS variables,
  one scoped CSS file and its assets, served read-only through a custom
  protocol with a CSP (no `@import`, no remote URLs). No theme JavaScript:
  on Linux Tauri can't tell an iframe from its window, so theme code would
  get romforge's own rights, moving and deleting files included. A
  versioned theme API with stable `data-*` hooks and variables, never
  internal class names (Steam Deck CSS themes broke en masse when Valve
  renamed classes). Live reload and an in-app editor for a theme's
  options (Big Box's and Playnite's XAML with no live preview is their
  users' recurring complaint).
- **Where themes live:** romforge's own themes (its own assets) in this
  repo; **community themes in a separate `romforge-themes` repo** with a
  `themes.json` index, listed, downloaded and updated from inside romforge
  (as ES-DE's theme downloader does). Not a long-lived branch of this
  repo, which would drift into a second app. Each theme's licence is shown
  before download and none is bundled: many frontend themes are
  CC BY-NC-SA and carry console logos (rules 3 and 10). ES-DE's theme
  format is documented and MIT, so reading its themes is possible later,
  but costly; parked.

**Steps, in order:**
1. *Controller core:* `controller.rs` (gilrs, Steam dedupe, d-pad and
   stick to directions with a held-key repeat, buttons to actions),
   events to the app; the desktop UI driven by a pad too.
   *(Built 2026-10-20.)* `controller.rs`: Steam dedupe (`usable`, with
   `SDL_GAMECONTROLLER_IGNORE_DEVICES` / `SDL_GAMEPAD_IGNORE_DEVICES`),
   buttons to actions by SDL's names (South = accept, East = back, Start =
   menu, Select = context, shoulders page, triggers jump; the guide button
   left to Steam and the system), d-pad and left stick to directions (the
   stick past 0.6, let go under 0.35) repeating 400 ms then every 110 ms.
   gilrs itself (`controller/pad.rs`) is behind the `controller` feature,
   which only the app turns on, so the CLI needs no udev. The app listens
   from start-up and sends "pad" events; the desktop UI follows them
   (`padnav.js`): in a dialog the d-pad moves the focus, A presses, B
   closes; in the library up/down move, A plays, shoulders switch
   platform, triggers jump to the first or last game, Start opens
   Settings, Select Emulator Settings; nothing while a game runs. Pad
   access: systemd's udev rules give the logged-in user joysticks. Not yet
   tried with a real pad (none on the dev machine), nor with Steam's
   virtual pad.
2. *Fullscreen mode:* platform carousel, game grid and game page with the
   built-in theme; Start for the main menu, Select for a context menu,
   North for per-game options, left/right to switch platforms, shoulders
   to page, triggers to jump; a help bar always on; `--fullscreen`.
3. *Hotkeys:* the table above, written into every emulator; the graceful
   quit chord.
4. *Theme engine v1:* manifest, variables, options, fallback variants,
   live reload; two or three first-party themes, SNES UK the default.
5. *`romforge-themes`:* the index, downloads and updates, licences shown.
6. *Extras:* favourites, recently and most played (from `play_sessions`),
   quick search with the on-screen keyboard, attract mode separate from a
   screensaver, background music (the user's files), per-game settings
   from the game page, startup and exit splashes per platform.
7. *Steam Deck:* `shortcuts.vdf` entry with art, Steam Input templates.

Not yet sourced: video snaps (no current art source has videos;
ScreenScraper does, keyed, so later as an art source); RetroAchievements
(a login token, optional, as before).

Sources (2026-10-20): ES-DE's THEMES.md and LICENSE (gitlab.com/es-de);
RetroDECK's wiki (hotkeys, known issues); EmuDeck's hotkey docs; Big Box
startup/pause/attract docs (launchbox.featurebase.app); Playnite theme
docs; DeckThemes CSS Loader docs and v2.1 notes; Tauri capabilities and
isolation docs; gilrs and SDL docs; WebKit bugs 160760 and 160699;
RetroArch's network control docs; PCSX2's `InputManager.cpp` and PINE;
gamescope-session; Valve's steam-devices rules. Unchecked and to verify
in source before use: gilrs with Steam's virtual pad, xemu's and Cemu's
hotkeys, DuckStation's PINE in current builds.

### Stage 3 (future): truly one window for everything

Planned, not scheduled: starts only after stages 1 and 2 are solid on the
real library. Every game then plays inside romforge's own window: no
fullscreen handoff, and the pause menu is romforge's UI drawn over the game.
Stage 1 stays the fallback for any emulator stage 3 can't host yet.

#### 3.1 One window that holds the library and the game

The Tauri webview can't take a game's frames directly (frames over IPC to
WebKitGTK cost latency and drop frames), so romforge's window becomes a
native winit window that both share:
- the game drawn with wgpu (or glow for OpenGL cores);
- the library and pause menu as a wry webview inside the same window
  (`build_as_child`), shown over the game or instead of it. To test early
  on Wayland.

Games run in a child process (`romforge-play`) that renders into a surface
romforge owns, so a crashing core or emulator can't take the library down.

#### 3.2 Built-in libretro player, software cores (approach B)

- `libloading` (ISC) loads a core; `rust-libretro-sys` (MIT) for the
  types. `retro-rs` exists but has no visible licence: check before use;
- audio with dynamic rate control (cores run at e.g. 60.098 fps), SRAM
  flushed on exit, save states through `retro_serialize`, input from
  `gilrs` mapped to the RetroPad, the same `Bios/`, `Saves/`, `States/`;
- covers 8/16-bit, handhelds, PS1 (software), arcade.

Reference frontends: Ludo (Go, libretro org, non-GL cores), nanoarch (one C
file), rustro_arch (small Rust tutorial), GameLord (Electron, frames to a
WebGL canvas).

#### 3.3 3D libretro cores (approach C)

The core draws into a GPU context romforge provides (`SET_HW_RENDER`):
create the context and its framebuffer, give `get_proc_address`, call
`context_reset` / `context_destroy` when the window is recreated (resize,
fullscreen), keep the core and its context on one thread. OpenGL first
(Mupen64Plus-Next, Flycast, Beetle PSX HW, PPSSPP, Dolphin), then Vulkan
(`libretro_vulkan.h`; LRPS2, Dolphin, paraLLEl). Where a core lags its
standalone emulator (Dolphin), the user can pick the standalone one.

#### 3.4 Standalone emulators inside the window (approach D)

For systems with no libretro core (PS3 / RPCS3, Wii U / Cemu, Xbox / xemu,
Vita / Vita3K, Switch, Xbox 360), and for any standalone emulator the user
prefers. On Wayland one program can't embed another's window, so romforge
runs a small nested Wayland compositor (Smithay, MIT) that the emulator
connects to as its display; romforge shows its output in the game area,
X11-only emulators through Xwayland. romforge then also owns input focus and
can draw its pause menu over the emulator. Gamescope does the nesting part
(in its own window) and is the model to study. No launcher found that does
this: an experiment first, on one emulator.

Licensing: romforge never ships cores or emulators. The user downloads each
one, so an MIT program loading a GPL core at runtime places no obligation
on romforge's distribution. Non-commercial cores carry their warning as in
1.4.

### Order with milestones 20–22 (decided 2026-10-19)

Stage 1 and milestones 20–22 are one piece of work, built in this order:
1.1 the emulator table, 1.2 the folders, 1.3 owned configs with
milestone 21's settings (RetroArch first), milestone 20's BIOS page
(replacing 1.5), 1.4 getting emulators, 1.6 start and exit, then each
standalone emulator's settings, and milestone 22 (Switch).

### Not planned

- Shaders and overlays in the built-in player.
- Copying RetroDECK's or EmuDeck's configs (GPL-3.0); only their ideas.

### Open

- Check every flag and config dir above on the real emulators (RetroArch,
  ScummVM, MAME and the DOSBox Flatpaks are installed on the dev machine
  now; the console emulators are not).
- Whether Flatpak emulators can take a portable config dir, or only
  `~/.var/app`.
- A licence for `retro-rs`, or write the small frontend layer ourselves
  (stage 3).
- Whether a wry child webview over a wgpu surface works on Plasma Wayland
  (stage 3.1), and whether a Smithay-nested emulator runs at full speed
  (stage 3.4).
- Whether RetroArch's UDP interface is enough for a pause menu (it saves
  only to the current slot).

## Patcher (milestone 19; decided 2026-10-19; built the same day, tested on scratch copies)

Apply a translation or hack to a game you have, making a new patched copy
and leaving the original as it is.

Checked 2026-10-19:
- romhacking.net went read-only in August 2024; its pages stay up but have
  no API and refuse automated requests (403). Its Internet Archive export
  (`romhacking.net-20240801`: SQL database and 12.5 GB of files) is
  private (401). Romhack Plaza documents no API or bulk data. So no source
  romforge could download from by itself without scraping.
- `patch.rs` already applies IPS, BPS and UPS in memory (to 64 MiB) to
  prove what a file is; BPS and UPS name their source's size and CRC32.
- Disc translations mostly ship as xdelta (VCDIFF), some as PPF.
  `oxidelta` (MIT) decodes VCDIFF in pure Rust; PPF is small enough to
  write here.

Decided:
1. *You fetch the patch, romforge applies it.* Details gets "Find a
   translation" (search links to romhacking.net and Romhack Plaza for the
   game, opened in the browser, like the other links) and "Apply patch…":
   a `.ips`, `.bps`, `.ups`, `.xdelta`/`.vcdiff` or `.ppf`, or a zip holding
   one (its readme kept with it). Patches found beside your ROMs (as Search
   already sees them) are offered too. No scraping.
2. *A new copy in RomForge, shown as a version.* The patched game is
   written into `Games/<System>/` as "<Game> [Translation: <tag>]" (or
   Hack), zipped or as a CHD like any import, journaled for undo; the
   original is never touched. It lists as a version under its base game,
   "patched by romforge from <patch>": proven (the patch applied to your
   verified copy gave it), never "verified" unless a hack DAT lists its
   hash (rule 8).
3. *Cartridges and discs.* Cartridges with IPS, BPS and UPS in memory, as
   today. Discs with xdelta and PPF, streamed: from the CHD (or loose
   tracks), patched, and written back as a CHD (or the patched track
   beside the others for loose discs).

To work out while building: which form of the ROM a patch wants (BPS/UPS
say it by CRC32: the raw file or the DAT's header-stripped form; IPS says
nothing: the DAT's form unless the patch's name says headered, and the
result is checked against hack DATs where there is one); which track of a
multi-track disc an xdelta patch targets; patches that need an older
revision than the one you have (refused, naming the one it wants).

As built: `patcher.rs` (plan, apply, patches beside a game), PPF 1-3 and
xdelta in `patch.rs` (oxidelta, MIT, its plain and LZMA secondary
compression; streamed for discs), `disc::read_chd` (a CHD's tracks to a
hasher or to files), `patched` (migration 31) for what made a copy,
`romforge-cli patch`, Details → **Patch…** and `patch_links`. Journaled as
a cleanup batch. Tested with an IPS on a headered NES ROM, a BPS (and one
for another ROM, refused), and an xdelta and a PPF on a two-track CHD.
Follow-up (2026-10-19, built and tested on scratch copies):
- `vcdiff.rs`: xdelta3's DJW and FGK secondary compression, ported from
  xdelta3 (Apache-2.0), rewriting such a patch as plain VCDIFF for
  oxidelta; its application header names the source file. Tested with
  patches xdelta3 made (`fixtures/patches/`; FGK single-window, as
  xdelta3's own multi-window FGK fails).
- Discs: the track an xdelta names, else the first that takes it.
- Cartridge games of several ROMs: BPS/UPS by their source, else by name,
  else the biggest (a note says so); the zip keeps the DAT's names.
- Patches join their game (migration 32: `patch_files`): a scan records
  the patch files it finds (loose, or a zip with one patch) and pairs
  each with a game by its source ROM, the file it names, a same-name file
  beside it, its names against your titles, or the title its readme
  names; translation or hack from its name and readme. Details → Patch…
  and `romforge-cli patch <game>` list them.
- Patched ROMs named on their own: an unknown cartridge ROM that a paired
  patch beside it makes from your verified copy of its game gets a
  derived name (`manual_ids.proof`), never verified; Forget puts it in
  `proofs_declined` so it isn't named again.
- Tried on a real patch (Ps2restIPS.zip, Phantasy Star II restoration):
  read with its readme, paired by the readme, applied to a stand-in ROM
  in scratch; not yet on the real game (not in the library).
- The list (2026-10-19): `view::listing` (the app's and `games`' view;
  `view::library` stays the plain one inside the core) puts a game's
  original on top, a placeholder row (status `missing`) when you have
  only translations, hacks or patches of it, and gives every other
  version a `Category` (region, edition, translation, hack, patch
  translation, patch hack) and `base_id` (the row it is made from). Patch
  files are rows (status `patch`); one for a game you don't have pairs
  with the DAT game its readme names (one platform, or the platform the
  readme names). The app folds each heading; a patch row's Apply opens
  the patcher on its game.
**Open:** discs aren't named on their own (only cartridges); a zip of
several patches and patches in 7z archives aren't listed (pick one by
hand); the new list layout is tested in the core, not yet looked at in
the running app.

## Details, Development, Series and sorting (2026-10-19, built)

- Details centred, its facts left of the box art.
- Development: Wikidata's developer, publisher, director, designer and
  composer kept as items (`credits`, migration 33) with each item's logo
  (P154) or image (P18) from Commons (`wikidata_pictures`), fetched by
  Enrich as 240 px thumbnails into the art folder (`wikidata/<qid>.png`);
  cards in the app link to the item and to the picture's Commons page.
  libretro-only names stay text.
- Series: each title once with the platforms you have it on; numbered
  entries apart from the rest.
- Sorting: every list (games with a release-year column, series, DATs, DAT
  games, Organize moves, Import items) by its headings, remembered per
  list; `view::sort_rows` for `romforge-cli games --sort`.
- Not checked yet with real Wikidata pictures (the test serves a stand-in).

## Fanart picker (2026-10-19, built)

- Every source now returns all its images of a kind, best first
  (`enrich::ArtFound`: URL, preview thumbnail, label); Enrich still keeps
  the first that downloads.
- `enrich::art_choices` asks libretro, the LaunchBox database and each
  keyed source for a game's pictures of a kind, downloads a preview of each
  (thumbnails where the source has them) into `<cache>/art-choices`;
  `choose_art` downloads the pick, keeps it as art and marks it `chosen`
  (migration 34, with each picture's `url`), and Enrich never replaces a
  chosen picture, even refreshing.
- The app: click the fanart in Details (or "No fanart yet: find one…").
  Only fanart for now; the core takes any kind, so box art, logos and
  screenshots can get the same picker.
- Not tried against the real sources yet (tested with a stand-in).

## Library views (2026-10-19, built)

- List, Box art, 3D box, Banner. Rows carry their picture files
  (`GameRow::pictures`: box art, 3D box, fanart, logo; translations,
  hacks, patches and missing originals borrow their game's).
- The app serves art through its own `art://` scheme (only image files
  under the cache and the RomForge folder's Media/), so a grid loads only
  what is on screen, straight from disk. Details, Development's pictures
  and the fanart picker's previews use it too (2026-10-19): sending a
  game's art as base64 (6.6 MB for one game here) froze the page for
  seconds on every pick.
- `PictureGrid.svelte`: virtualised lines of cards; headings and versions
  as in the list. Arrow keys move one card at a time.
- Box shapes (2026-10-19): `systems::box_aspect(system, BoxRegion)`, a
  short table of known shapes by platform and region (North America,
  Europe, Japan) and a rule for the rest; `GameRow::box_aspect`. Cards are
  one size (the widest box in view), boxes one height on a baseline, scans
  fitted inside (no crop). 3D boxes are drawn 15% wider for the spine.
- Pictures only (2026-10-19): no card or title behind a picture (the name
  on hover, a placeholder with the name when there's no picture); banners
  all 3.2:1, the fanart cropped to fill. Details' facts in two equal
  columns. A Versions tab after Series: the game's group under the list's
  headings, in any of the four views (`GameTable.svelte` and
  `versions.js` shared with the library list).

## Toolbar: Scan folder, Rename & Cleanup, Convert (2026-10-19, built)

- Refresh (`refresh_settings`): only Source, Library and RomForge's Games;
  `refresh` (CLI, and after a cleanup, undo or convert) still takes every
  scanned folder. Greyed out until there is a folder (`can_refresh`).
- Scan folder (`scan_folder`): any folder, not kept as a setting, except
  the first while no Source is set, which becomes the Source. Its dry run
  (`scan_preview`) scans a `VACUUM INTO` copy of the library in the temp
  folder, lists the rows with files in that folder (new or not), and
  deletes the copy; accepting scans for real (the files are read again).
- Rename & Cleanup: runs at once; its dry run is the old plan dialog.
  Settings: move then trash (default), move and keep, or replace in place
  (`Settings::in_place`: written in the game's own folder, the original to
  the trash; arcade sets and folder games stay where they are).
- Convert (`cleanup::convert_plan`): identified disc images, not yet CHD,
  into a CHD beside themselves under their own names, no sidecar; only
  platforms whose extensions include `chd`. Cleanup now also skips CHD
  for the others (it used to write GameCube and Wii ISOs as CHD, which
  Dolphin can't read).

## Platform icons and Settings tabs (2026-10-19, built)

- `enrich::platform_icons`: controllers (`flatui`, the default) or
  systems (`systematic`) from RetroArch's menu icons (retroarch-assets,
  CC BY 4.0, house rule 10 tier 1) by the platform's libretro name, or
  logos (`logo`) from Wikidata (P154 of the platform's item, a 256 px PNG
  Commons renders); `Settings::platform_icons`, fetched once into
  `<cache>/platform-icons/<style>/`; a missing one is remembered.
  Platform names in the sidebar wrap whole, in bold. Shown through `art://`; a platform without one keeps its
  coloured badge.
- Settings in tabs: Folders (the Source now shown with a RomForge folder
  too), Rename & Cleanup, Display (icons, regions, empty media boxes),
  Sources (keys, ScummVM), Credits.

## Caching for big libraries (2026-10-19, built)

Measured on a synthetic library of 20,000 verified games (release build):
building the list view 0.9-1.0 s, and it was rebuilt after any write at
all (a rescan finding nothing new, Enrich's bookkeeping), and the whole
7.6 MB view went to the page after every action.

- `view::fingerprint`: a hash of everything the list shows (each table it
  reads, only the columns it uses, through an `fp()` SQL function), worked
  out only after a write (a few tens of ms); the list is rebuilt only when
  it changes. Rescan with nothing new: 0.97 s to 48 ms.
- A snapshot of the view beside the database (`<db>-view.json`), keyed by
  the fingerprint, the database's path and the romforge build: opening
  the library reads it (81 ms) instead of building (1 s).
- The page asks with the generation it has (`library_view_since`) and gets
  the view only when it changed.
- Tried first and dropped: triggers bumping a counter on every write to
  those tables made a 20,000-file scan 2.0 s to 7.1 s (migration 35 now
  removes them from databases that got them).
- Thumbnails for the picture views (`thumbs::thumbnail`, the `image`
  crate with PNG, JPEG and WebP only): `art://…?h=<px>` serves the
  picture scaled to the height the grid draws it at (box 170 px, a
  banner's fanart as tall as a 16:9 picture the row's width; times the
  screen's density, in 60 px steps), made on first request in the
  scheme's own thread and kept in `<cache>/thumbs/`, named by the path,
  size, modification time and height (a changed picture gets a new one).
  JPEG q85 when opaque, PNG with see-through parts; a picture no taller
  than asked is served as it is; any error serves the original. Details
  keeps the full pictures. Measured on 3,000 box scans (512×720), 1,500
  3D boxes (800×1000, alpha) and 750 fanarts (1920×1080), release build,
  noisy synthetic pictures: decoding per card 41 → 3.9 ms (box), 28 → 4.8
  ms (3D, 1.6 MB → 173 kB), 14 → 8.2 ms (fanart at 780 px, little gained
  on a wide window); making one on first view 32 / 40 / 115 ms; finding
  it again 5-19 µs. Old thumbnails of changed pictures stay until the
  cache is cleared.

## Wordmark and app icon (2026-10-19, replaced the logos)

The five pasted logo images, the Logo setting (`Settings::logo`, its
stored key cleared on the next save) and the traced RF icon are gone.
A modern take (ROMFORGE in Audiowide, an "RF" icon from its outlines) was
tried first; the owner chose a colourful retro look instead:
- **Wordmark** (`Wordmark.svelte`, toolbar and first-run screen):
  original pixel lettering, 7×7 cells a letter with chunky strokes and an
  L-shaped highlight in each letter's top-left corner (after the owner's
  reference, which a font identifier matched to Rocabe and Square Byte,
  both commercial; so it is drawn, not set in a font), each row a stripe
  from yellow through orange and pink to purple, with a deep purple drop
  shadow. Drawn as SVG cells: sharp at any size, nothing to license.
  Recoloured the same day to the UK (PAL) SNES's colours: each letter
  in a face-button colour in the buttons' order (red, yellow, green,
  blue, twice), darker at its foot, with a console-grey shadow.
- **App icon** (`icons/romforge.svg`, rendered to the bundle's PNG
  sizes): original 32×32 pixel art on a transparent background: "ROM" in
  the wordmark's letters, R red, O yellow, M green, standing on an anvil
  in shades of the blue button, sparks flying. (Drafts first had no text:
  a cartridge, then a SNES-style cart on the anvil; the owner swapped the
  cart for the word.)
- **App theme** (`app.css`): the same palette, console greys for
  backgrounds and panels, the blue button as the accent, green, yellow
  and red for OK, warning and error (lighter shades in dark mode).
The font-recommendation and font-identification tools gave the direction
(squared, blocky, arcade); every font they named was commercial, which is
why both pieces are hand-drawn. Platform and genre icons are unchanged
(features, not branding).

## 400 lines per file (2026-10-19, built)

Every code file, tests and the app included, is at most 400 lines (house
rule 11). The long ones were split into child modules behind the same
root files, so public paths are unchanged; App.svelte became components;
the long test files became smaller test binaries with shared helpers in
`tests/common/`. Only the systems table (`systems/table.rs`) is exempt.
`tests/file_length.rs` fails CI on a file over the limit.

## Firmware (milestone 20; researched 2026-10-19, not built)

romforge knows which core runs a system but not whether that core has the
BIOS it needs: a PS1 game "launches" and RetroArch stops at a missing
`scph5501.bin`. RetroBIOS (github.com/Abdess/retrobios, site
abdess.github.io/retrobios) tracks, from each emulator's own source, every
file 530 emulators and 12 frontends load: name, where it goes, required or
optional, size, CRC32, MD5, SHA1.

**What is used and what never is.** Its metadata (scripts, schemas,
emulator profiles, platform lists, the generated database) is MIT and
served as versioned static files with no account or key
(`/retrobios/api/v1/`: `platforms.json` 2.5 MB, `emulators.json` 5.6 MB,
`emulators/<id>.json`; also a SQLite snapshot, CSVs and a Logiqx
"collection DAT", each with a published SHA256). That is tier 1 of house
rule 10, fetched like DATs (1 s per host). The BIOS files themselves (the
`bios/` tree, release packs, `install.sh`/`install.py`) are third-party
system software it redistributes without a licence: romforge never
downloads, links, ships or runs them (rules 7 and 10). It checks and
places the user's own dumps by the hashes, nothing more.

**Two kinds of "good".** RetroBIOS hashes say what an emulator's code
accepts, not that a file is a verified dump; only 563 of its 5,339 system
files are in No-Intro, Redump or TOSEC. So a firmware file matching by
size + SHA1/MD5 (rule 8) is "accepted by <core>", its own status, never
counted as DAT-verified. RetroArch only checks that the name exists, so a
right name with a wrong hash is shown as "present, hash unknown".

In this order:
1. *Fetch the requirements.* `firmware download` (and the BIOS page)
   stores, per emulator: its cores, systems, and each file (name,
   destination, required, size, hashes, archive contents for MAME-style
   BIOS zips). RetroBIOS system ids (`atari-7800`, `sony-playstation`)
   map to romforge systems through a new `retrobios` column in
   `systems/table.rs` (rule 4), checked against
   `platforms/_registry.yml`. Profiles carry the `source_commit` they were
   read from; stored with the rows, refreshed when `generated_at` changes.
2. *A BIOS page,* in the sidebar next to DATs, built like the DATs view:
   per system and per core, required and optional files with their state
   (accepted / present, hash unknown / missing / wrong hash), where each
   one goes, and a badge on the platform when its first core lacks a
   required file. CLI: `firmware check [--system X] [--core Y]`.

   *(Steps 1 and 2 built 2026-10-19.)* `firmware.rs`:
   - **List:** `firmware download` (and the page's button) keeps
     `api/v1/emulators.json` (534 profiles, 5.6 MB) as
     `Databases/retrobios-emulators.json`, read whole each time (no tables
     or migration). Read leniently (`firmware/data.rs`): a hash is one
     value, a list or comma-separated; aliases fold into their target's
     cores. Profiles cover 82 of romforge's 85 cores and emulators (not
     yet: `o2em`).
   - **Systems:** the `retrobios` column lists each system's RetroBIOS ids
     (106 systems, every id checked against the list; several spellings
     each). Arcade systems have none: MAME's DATs cover their BIOS sets.
     A file tied to a RetroBIOS system is listed only for its systems; a
     file tied to none, for every system of its emulator.
   - **Check:** each file at its place, `Bios/RetroArch/<path>` for
     cores, `Bios/<emulator>/<path>` for the others (read once per run).
     States: accepted (size + SHA1/MD5), present (no hash listed), wrong
     file, missing.
   - **Ready:** every required file without an HLE fallback is there,
     except that files of one `variant_group` are alternatives and
     regional files (`region`) need one region's: "ready for
     north-america". DuckStation's three regional BIOS files are then one
     requirement, not three.
   - **Page:** BIOS in the sidebar under DATs: platforms with "ready n /
     m", each emulator's files with state, need, region and where it goes.
     CLI `firmware check [--system X] [--all]`.
   - **Not yet:** the platform badge in the library sidebar; `--core`;
     archive members (a BIOS zip is checked as a file); checking files
     outside `Bios/`.
3. *Import BIOS files by dropping them on the page* (or "Add BIOS…", or
   `firmware import <path>`). Dropping a folder or an archive starts the
   process; Tauri's webview drag-drop event gives the paths, and the app
   only passes them to the core (rule 1).
   - **Reading:** folders, zip, 7z, a zip inside a zip (packs nest them),
     several ordinary zips at once (a pack in parts). Read as streams by
     the readers romforge has (rule 7); BIOS files are small, so nested
     members are read in memory, never extracted to disk. Slices
     (`.zip.001`) are refused: none opens on its own.
   - **Matching** by contents, never by name: size + SHA1/MD5 against
     step 1 (rule 8), so a pack that renamed everything still works. Each
     file is *accepted* (romforge knows every system, core and name it
     serves), *name fits, hash unknown* (not placed unless the user picks
     it; then "placed by you", never "accepted"), an *arcade BIOS set*
     (`neogeo.zip`: goes to milestone 17's arcade import, MAME's DATs
     cover it) or *unknown* (listed, left alone).
   - **Plan first:** what goes where, which systems and cores become
     complete, what is still missing, what was skipped. Nothing written
     until the user confirms (rule 6).
   - **Apply:** copied, never moved (the dropped pack stays as it was),
     into `RomForge/Bios/` (milestone 18's folder):
     ```
     RomForge/Bios/
       RetroArch/            laid out exactly as RetroArch's system folder
         scph5501.bin
         dc/dc_boot.bin
       duckstation/  pcsx2/  ...   one per standalone emulator whose
                                   layout differs
     ```
     One file needed under several names or folders is stored once and
     hard-linked (same drive). Each new file is re-hashed, nothing is
     overwritten, the batch is journaled, and "Undo" removes it.

   *(Built 2026-10-19.)* `firmware/walk.rs` reads a drop (folders all
   the way down, files, zip and 7z with archives inside them one level,
   nested ones up to 256 MiB in memory) as streams; the same walk plans
   and copies. `firmware/import.rs`:
   - **Plan:** each file hashed and matched against every place a core
     or emulator of a romforge system loads it from (`Bios/RetroArch/…`,
     `Bios/<emulator>/…`), with who loads it. Per place: goes there /
     already there / another file there (left alone) / another dropped
     file goes there.
   - **Apply:** writes each file once (create-new, never replacing),
     re-hashes it, hard-links its other places (a copy where a link
     fails), and journals every file it makes (migration 36,
     `firmware_batches` / `firmware_outputs`); "Undo last import" removes
     them. Name-only files are placed only when ticked.
   - **App:** the BIOS page takes drops (Tauri's webview drag-drop event,
     paths passed to the core), plus Add folder… / Add files…; it shows the
     plan and imports on "Import (n places)". CLI: `firmware import PATH…
     [--apply] [--also NAME]`, `firmware undo`.
   - **Not yet:** a needed BIOS zip dropped whole (e.g. `neogeo.zip`) is
     opened, not placed as a zip; "placed by you" isn't shown on the page
     afterwards (a name-only file then reads as present).
4. *Point the emulators at it.* Each emulator's BIOS path is set in
   romforge's own copy of its config (milestone 18, 1.3): RetroArch's
   `system_directory`, DuckStation's BIOS search folder, PCSX2's BIOS
   folder, and so on. An emulator with no BIOS-path setting keeps BIOS
   files inside its data folder, which romforge owns too
   (`RomForge/Emulators/config/<emulator>/`), so they are hard-linked
   there from `RomForge/Bios/`. Nothing is written outside RomForge. A
   Flatpak that can't read `RomForge/` is reported with the access it
   needs.
   *(Built 2026-10-19 for seven emulators.)* `launch/owned/standalone.rs`,
   each read from its source (keys and folders in the file's header):
   DuckStation (`XDG_CONFIG_HOME`, `[BIOS] SearchDirectory`), PCSX2
   (`-datapath`, `[Folders] Bios`), mGBA (`XDG_CONFIG_HOME`,
   `[ports.qt] gba.bios`… for files there are), Mednafen
   (`MEDNAFEN_HOME`, `filesys.path_firmware`), Flycast (XDG folders,
   `FLYCAST_BIOS_PATH`), Dolphin (`-u`; files linked into the user folder
   where it reads them, as it has no key) and PPSSPP (no BIOS). Each also
   gets the keys that keep its first-run wizard from opening or its
   settings from being reset (DuckStation's and PCSX2's wizard flags,
   PCSX2's `SettingsVersion`). The BIOS check and import use RetroBIOS's
   `standalone_path` for a standalone emulator where it gives one
   (Dolphin's `GC/<region>/IPL.bin`). Writers for INI and Mednafen's
   `key value` files are in `owned/formats.rs`; environment variables
   reach a Flatpak as `--env=`.
   *(Decided and built 2026-10-19: RPCS3, Cemu and xemu.)* They get
   romforge's own config like the others, but their game data stays where
   it is and romforge's config points at it (`launch/owned/gamedata.rs`,
   read from their source): RPCS3's `vfs.yml` devices at the user's own
   `dev_hdd0`, `dev_flash`… (their usual folder, or where their own
   `vfs.yml` moved them), its welcome dialog off; Cemu's `settings.xml`
   `<mlc_path>` at the user's MLC (installed titles, updates, DLC,
   saves), the file always written so "Getting started" doesn't open,
   `keys.txt` / `otp.bin` / `seeprom.bin` linked from `Bios/cemu/`;
   xemu's `xemu.toml` with `show_welcome = false`, the user's own
   `hdd_path` and `eeprom_path` (from their `xemu.toml`), else
   `Bios/xemu/`'s, and `bootrom_path` / `flashrom_path` from
   `Bios/xemu/`. PS3 firmware isn't a file to place: "Install into
   RPCS3" on the BIOS page (`firmware install-ps3`) runs RPCS3's own
   `--installfw … --headless` with romforge's config and checks the
   firmware landed (RPCS3 exits 0 even when it fails). Writers for TOML,
   flat YAML and Cemu's XML joined `owned/formats.rs`. Not yet: RPCS3's
   Flatpak still asks about being an unofficial build (no setting); their
   game data isn't moved into RomForge (a later "move it in" step).
   *(Built 2026-10-19: "Import my existing setup".)* An emulator started
   from romforge's folder no longer sees its usual one, so Emulator
   Settings (each emulator's tab) and `emulator import-setup NAME
   [--undo]` copy it across (`launch/owned/existing.rs`): the usual
   folders from each emulator's source, native and Flatpak
   (`~/.var/app/<id>/config|data`), caches and logs left behind. Files
   already in romforge's folder are kept, except the emulator's settings
   file, which becomes the base (romforge's keys go back in at the next
   start; the one replaced is kept beside it as `.romforge-before-import`).
   Journaled (migration 37); undo removes the copies and restores that
   file. RPCS3's devices and Cemu's MLC aren't copied (romforge points at
   them). RetroArch: its `retroarch.cfg` only, so its controller setup
   comes along and its autoconfig, assets and remaps are read where they
   are; its core-sorted saves aren't mapped to romforge's per-system
   folders yet.
5. *Launch.* Before starting a core, check its required files. If they're
   missing, try the next core in the `cores` column whose files are
   complete (or that needs none, per its profile: an HLE BIOS), else say
   which file is missing and that the BIOS page takes it, and start
   nothing. `--dry-run` shows the check.
6. *BIOS found among games.* Loose BIOS files in scanned folders show as
   unknown today. Files matching RetroBIOS system files by size +
   SHA1/MD5 become "firmware for <system>" rows instead, with "Import to
   BIOS" (system files only: its arcade sets overlap MAME's DATs and its
   game-engine data is out of scope). The Redump BIOS DATs keep verifying
   where they cover.

Not used: the `provenance/` catalog matches (No-Intro / Redump / TOSEC
already verify those files directly); the per-frontend pack layouts
other than RetroArch's (romforge launches RetroArch and standalone
emulators, whose BIOS folders come from their profiles in
`emulators.json`).

## Emulator settings (milestone 21; decided 2026-10-19, not built)

romforge has full control of its own copy of each emulator's config
(milestone 18, 1.3: `RomForge/Emulators/config/<emulator>/`), so users
set up every emulator inside romforge instead of in each emulator's own
menus. The emulator's own config, used when it is started outside
romforge, is never touched. Milestone 20 uses this to set BIOS paths.

- *One table of emulators' config files* (like `headers.rs`): how to
  point the emulator at romforge's config folder (a flag, a portable
  marker, a Flatpak data path), its format (RetroArch
  `key = "value"` cfg, INI for DuckStation / PCSX2 / Dolphin, ...) and
  the settings romforge offers: BIOS and save folders, video (renderer,
  resolution scale, fullscreen), input, per-core options
  (`retroarch-core-options.cfg`), each with its type and allowed values.
- *Emulator Settings* (toolbar, next to Settings) to view and change
  them: a Global tab for every emulator at once, then a tab per installed
  emulator, each taking the global value unless it sets its own.
  *(Built 2026-10-19, first part.)* `emusettings.rs` holds the one table
  of settings (`SETTINGS`: key, label, kind, whether it can be global,
  default), stored as `emu.<global|emulator>.<key>` in `settings`. The
  dialog draws whatever the table holds. Today's rows are the ones
  romforge applies at launch: Fullscreen (global, overridable), Program
  (replaces 1.1's `emulator_paths`) and Extra arguments (per emulator).
  MAME's machine entries share MAME's tab (`shares`). CLI: `emulator
  settings`, `emulator set <global|emulator> KEY [VALUE]`. Not yet seen
  in the running app (checked by build and tests only). The settings
  written into romforge's config copies come with 1.3, as more rows:
  RetroArch's are in (see 1.3). A row lists the config keys it is
  written to per emulator; the global tab shows which installed
  emulators each one reaches.
- *Writing safely:* a new copy starts from the emulator's defaults
  (or, if the user asks, from a one-time read of their existing config).
  The file is parsed and rewritten keeping keys and comments romforge
  doesn't know, in order; the write is atomic (temp file + rename);
  every change is journaled with its old value, so one change or a whole
  batch can be undone. Not written while
  the emulator is running (it would overwrite on exit): romforge waits or
  says so.
- *Per game* where the emulator supports it (RetroArch per-game
  overrides, DuckStation game settings), from the game's detail panel.
- *Order* (decided 2026-10-19): RetroArch first (`retroarch.cfg`, core
  options, per-game overrides), then every mature, current emulator.
  "Mature and current" means a stable release, used for its system, and
  a release or steady commits in the last 12 months; checked when each
  one is started, and an emulator that stops qualifying keeps working
  but gets no new settings. Starting with the standalone emulators
  romforge already launches (milestones 5, 14, 17: DuckStation, PCSX2,
  Dolphin, PPSSPP, Flycast, mGBA, melonDS, xemu, RPCS3, Mednafen, Cemu,
  DOSBox Staging, DOSBox-X, ScummVM, MAME), then any other emulator that
  qualifies, added to the launch table and the config table together.
- *Exception: discontinued but well known* (decided 2026-10-19). An
  emulator no longer developed is still supported when it was widely
  used and the community still has its builds (yuzu, Ryujinx, Citra,
  ...). Its entries are pinned to its last release, which then never
  changes. romforge never downloads it (its official downloads are gone,
  and mirrors can't be trusted): it is only found where the user
  installed it (on `PATH`, Flatpak, AppImage, or a path set in
  Settings). Current emulators can be fetched from their own projects
  (milestone 18, 1.4); romforge never ships any.
- Each config format and key is checked against the emulator's source
  (pinned to a release), never written from memory.

## Nintendo Switch (milestone 22; decided 2026-10-19, not built)

Switch is a system already (`nsp`, `xci`, no DATs, no emulator). It gets
launch, emulator settings (milestone 21) and its keys and firmware
through the BIOS page (milestone 20).

- *Which emulators.* Yuzu and Ryujinx stopped in 2024; their forks
  change fast, and Nintendo sent a DMCA notice covering about thirteen
  of them in February 2026. Checked in their source 2026-10-19:
  - **Eden** (yuzu fork, git.eden-emu.dev): alive, v0.2.1 (2026-06-01),
    commits daily; program `eden`, Flatpak `dev.eden_emu.eden`; `eden
    [-f] [-g] <file>`.
  - **Citron**: its old host is gone; github.com/citron-neo/emulator
    carries it on (release 2026-04-27, commits into October); program
    `citron`, `-f`, `-g <file>`, no Flatpak.
  - **Ryubing** (Ryujinx fork): its source host didn't answer; its
    Flathub app was taken down by a Nintendo DMCA notice on 2026-06-30.
    Command line and folders unverified.
  - **yuzu** (last: mainline-0-1734, 2024-03-04) and **Ryujinx** (last
    archived master 2024-09-15): `yuzu -f -g <file>`; `Ryujinx -f
    <file>`, `-r <dir>` for its data folder.
  - The yuzu family keeps its data in `./user` of the **working
    directory** on Linux (else `$XDG_DATA_HOME/<name>`), so romforge
    starts them in their RomForge data folder.

  Every
  fork that meets the milestone 21 rule, checked when this starts, plus
  yuzu's and Ryujinx's last releases under its discontinued exception.
  Each one's config, keys and NAND folders and command line are read
  from its own source, pinned to a release. They are entries in the
  launch and config tables like any emulator, so one that disappears is
  dropped without touching the rest.
- *Keys and firmware: supplied by the user, handled like every BIOS
  file.* Dropped on the BIOS page with anything else, recognized,
  shown in the plan and placed where each installed emulator needs them,
  copied, journaled and undoable as in milestone 20. The limits:
  romforge never downloads, links, ships or derives them, never points
  to a site that hosts them, and never decrypts anything (decryption is
  what Nintendo's case against yuzu was about), except to compress with
  the user's own keys (milestone 23, owner's choice 2026-10-19). So
  recognition reads only what is plain:
  - **Keys** (`prod.keys`, `title.keys`): by format, lines of
    `name = hex` with the known key names. There is no hash list for
    them (they are per console in part, and romforge keeps no key
    values or key hashes), so they show as "Switch keys, recognized by
    format", not "accepted". Placed in `RomForge/Bios/switch/keys/`
    and hard-linked into each emulator's keys folder inside its
    romforge-owned data folder (yuzu family: `keys/prod.keys`; Ryujinx
    family: `system/prod.keys`; checked in source).
  - **Firmware**: a folder or zip of `.nca` files with 32-hex names,
    recognized by that shape (an NCA's contents are encrypted).
    Kept in `RomForge/Bios/switch/firmware/` and placed as each
    emulator stores installed firmware in its NAND, inside its
    romforge-owned data folder (yuzu family: the
    files in `nand/system/Contents/registered/`; Ryujinx family:
    `bis/system/Contents/registered/`, checked in source; Ryujinx's
    `<name>.nca/00` nesting still to confirm). That is copying files, as their own "Install
    firmware" does, with no decryption.
  - The BIOS page shows Switch per emulator as keys yes/no and firmware
    yes/no, with the firmware version when the emulator reports it.
- *Games.* `nsp` and `xci` are hashed as today; they stay unidentified
  until a free Switch DAT source is found (No-Intro's public mirror and
  Redump: to check). Header ID reading is limited to fields stored
  unencrypted, never anything that needs a key. Search and "identified
  by you" work as for any system.
- *Launch:* the fork's command line for one game, fullscreen; a game
  folder with updates and DLC is left to the fork (each installs them
  into its NAND).

## Native compression (milestone 23; decided 2026-10-19, not built)

Every large format whose emulators load a compressed form gets a writer
in the core, like CHD today (the `chd` crate's encoder): no chdman,
DolphinTool, nsz or other program, nothing for the user to download
(rule 7). A new romforge-only format is not made: emulators couldn't
load it, and each system's own format already compresses best. The
same guardrails as CHD: stream to stream, the new file read back and
re-hashed against the DAT before the original goes to the trash,
journaled for undo, and offered by Convert and by Cleanup for the
platforms whose emulators read it (as `chd` is today).

Each writer is written from its published spec, never ported from GPL
code (rule 9: Dolphin and Azahar are GPL); MIT-licensed reference code
(ZArchive, nsz) may be read and ported. Compression: the `zstd` crate
(MIT/Apache).

**Decryption only to compress** (owner's choice 2026-10-19; milestone
22's never-decrypt rule otherwise holds). Some formats only compress
well, or only exist, after decryption: Wii partitions in RVZ, NSZ /
XCZ, and WUD / WUX to WUA. For those three, and nothing else:
- keys come only from the user, dropped on the BIOS page (milestone 20);
  romforge never ships, downloads, links or derives a key, and keeps
  no key values or hashes in its code or data;
- a key is right when the data it decrypts passes the format's own
  check (Wii: the partition's hash tree; Switch: the NCA header's
  magic; Wii U: the TMD's content hashes); otherwise the game is
  reported and left alone, never written half done;
- decrypted bytes exist only in memory and inside the new compressed
  file, never as a loose decrypted file on disk;
- scanning, identification, launch and the BIOS page stay without
  decryption: a game that isn't converted is never decrypted;
- without the keys, Convert still offers what needs none, and says
  which key a game is waiting for.

In this order:

1. **RVZ** (GameCube, Wii), from Dolphin's `docs/WiaAndRvz.md`. Header
   with the ISO size and the disc's first 0x80 bytes (already read, 12.9),
   then groups compressed with zstd, and RVZ's packing of Nintendo's
   junk padding (a PRNG the reader re-creates, so it costs almost
   nothing). GameCube discs are unencrypted: the whole disc goes in
   raw data entries, no key needed. Wii partitions are encrypted; they
   are stored as Dolphin does, decrypted with hashes stripped
   (`wia_part_t`), using the Wii common key the user supplies. Without
   it, Wii discs aren't offered (raw entries would barely shrink).
   Convert and Cleanup then stop skipping GameCube and Wii.
   The re-hash reads the new RVZ back in full, re-creating hashes and
   re-encrypting Wii partitions, and hashes the ISO it gives against the
   DAT. The same reader lets the scan verify RVZs the user already has
   (today *Likely: compressed image*, 12.9); a Wii one needs the common
   key, since reading it back means re-encrypting, not decrypting.
2. **NSZ / XCZ** (Switch, after milestone 22), from nsz's
   `docs/formats.md` (MIT). An NCZ keeps the NCA's first 0x4000 bytes,
   then a table of sections (offset, size, crypto type, key, counter)
   and a zstd stream of the *decrypted* sections. Writing it decrypts
   with the user's `prod.keys` (header key, key area keys) and the
   title key from the game's ticket or `title.keys`. The re-hash
   re-encrypts each NCZ back into its NCA and checks it against the
   original NCA's hash (an NCA's name is the start of its SHA-256), so a
   converted game is provably byte-identical. An XCZ is an XCI whose
   NCAs became NCZs (the XCI's HFS0 partitions rebuilt around them).
   AES-CTR and AES-XTS from the RustCrypto crates (MIT/Apache).
3. **Z3DS** (3DS: `.zcci`, `.zcia`, `.zcxi`, `.z3dsx`), with milestone 24.
   Azahar's format (added in Azahar 2123): the ROM in seekable zstd
   (frames plus a seek table in a final skippable frame, zstd's
   `contrib/seekable_format`). Its header is read from Azahar's source,
   pinned to a release. It compresses the file's bytes as they are, so
   nothing is decrypted; encrypted dumps shrink little, decrypted ones
   by about a third to a half.
4. **WUA** (Wii U), with milestone 25. Cemu's ZArchive (MIT-0): zstd in
   64 KiB blocks with random access, one folder per title named
   `<16-hex title ID>_v<version>`, holding game, update and DLC
   together. It stores decrypted files. From a folder that is already
   decrypted (`code/`, `content/`, `meta/`) it needs no key. From a
   WUD / WUX it decrypts the game partition with the disc key from the
   user's Cemu `keys.txt` (and the Wii U common key, also the user's),
   checks each content against the TMD's hash, and packs the files.
   The re-hash compares each file in the new WUA with the same file
   read from the source. A WUA can't be turned back into the WUD, so
   undo is the original kept in the trash, and the plan says so.

Not planned: PS3 and Xbox 360 (RPCS3 and Xenia load no compressed
image), original Xbox (xiso trims, it doesn't compress), CSO / ZSO
writing (CHD covers PSP and PS2).

## Nintendo 3DS (milestone 24; decided 2026-10-19, not built)

3DS is a system already (`3ds`, `cia`, `cci`, `cxi`; No-Intro DATs,
"Nintendo 3DS" and "New Nintendo 3DS") but has no emulator, no ID
reading, and no compressed format. It gets:

- *Emulator.* Azahar (Citra's successor, current releases) through the
  launch and config tables and milestone 21's rule; Citra's last release
  under the discontinued exception. Command line, config folder and
  key file location read from Azahar's source, pinned to a release.
- *Formats.* Add `3dsx` and the Z3DS extensions (`zcci`, `zcia`,
  `zcxi`, `z3dsx`) to the systems table; a Z3DS file is hashed as the
  ROM inside it (streaming zstd), so it matches the DAT like a zip
  member.
- *ID reading* (`ident.rs`), from plain header fields only: a CCI's
  NCSD header (magic `NCSD` at 0x100) and the NCCH partition's product
  code (`CTR-P-XXXX`) and program ID. Offsets checked against 3dbrew
  before building. CIA: the TMD's title ID. The product code joins
  No-Intro serials like other cartridges.
- *BIOS page* (milestone 20): Azahar's system files and `aes_keys.txt`
  recognized by format only, placed like Switch keys; never
  downloaded, linked or derived.
- *Compression:* Z3DS (milestone 23, 3).

## Wii U (milestone 25; decided 2026-10-19, not built)

Wii U is a system already (`wud`, `wux`, `rpx`, Cemu in the launch
table, no DATs). It gets:

- *Emulator.* Cemu through the config table and milestone 21's rule;
  its command line (`-g <file> -f`, unverified) and portable folder read
  from its source, pinned to a release.
- *Formats.* Add `wua` to the systems table, and decrypted game folders
  (`code/`, `content/`, `meta/`) as folder games like DOS ones. A WUA is
  read with ZArchive's layout: its title folders give the title ID and
  version without decompressing everything.
- *Identification.* No free Wii U DAT known (Redump's: to check). ID
  reading from plain fields: the WUD's product code (`WUP-P-XXXX`) at the
  start of the disc, a WUX's header (magic `WUX0`), and `meta/meta.xml`
  in folders and WUAs (title ID, product code, names). Offsets checked
  before building. Search and "identified by you" as for any system.
- *BIOS page:* Cemu's `keys.txt` (disc keys) and the Wii U common key
  recognized by format only, placed in Cemu's romforge-owned folder;
  never downloaded or derived. Used by romforge itself only to compress.
- *Compression:* WUA from decrypted folders, and from WUD / WUX with
  the user's keys (milestone 23, 4).

## Manuals (milestone 26; researched and decided 2026-10-19; first part built 2026-10-20)

A game's manual, read in a Manual tab next to Details or in a window of
its own. Neither downloaded database has manuals (LaunchBox's 33 image
types have none; OpenVGDB only links GameFAQs). Manuals are copyrighted
scans, so they are treated like rule 10's unlicensed data: fetched or
added by the user, never shipped, never in git or fixtures.

Sources, in this order:

1. *Your own files.* A PDF dropped on the Manual tab belongs to that
   game (like BIOS import). "Import my existing setup" also reads ES-DE's
   `downloaded_media/<system>/manuals/<ROM name>.pdf` and LaunchBox's
   `Manuals/<Platform>/`, matched to games the way their art is.
2. *Vimm's Lair Manual Project* (about 6,200 manuals, 28 systems), found
   through Hasheous: `GET hasheous.org/api/v1/Lookup/ByHash/sha1/<sha1>`
   needs no key or account (tried: Super Mario World (USA) gives the
   attribute `VIMMManualId` 4770). The match is by hash, so house rule 8
   holds. The id is stored per game; Enrich asks once per game, requests
   1 s apart.
   - **Download, only on a click.** The tab's "Download manual (Vimm's
     Lair)" button fetches `https://dl.vimm.net/?manualId=<id>&category=pdf&destDPI=<dpi>`
     (the fields of the page's own form), DPI picked in the tab (default
     300; the page allows 50–640). One manual per click, never in bulk or
     from Enrich: `dl.vimm.net/robots.txt` disallows automated fetching.
     A "Visit Vimm's Lair" link beside it credits them. Not yet checked
     (robots.txt): whether the server also wants a Referer or cookie, and
     whether the page's script adds fields.
   - **Fallback: the page in a romforge window.** If the direct download
     fails or is not a PDF, `vimm.net/manual/<id>` opens in a separate
     webview (Tauri's `on_navigation` and `on_download`, both in tauri
     2.12 / wry 0.57 on WebKitGTK). That window has no capability (no
     romforge commands, no files); it may load only `vimm.net` and
     `dl.vimm.net`, other links go to the browser; the user clicks the
     page's own Download pdf. Only downloads from `dl.vimm.net` are kept.
   - **Either way:** saved into staging, kept only if it starts with
     `%PDF`, moved to `Media/<System>/manual/<Title>.pdf`, and recorded
     with source "Vimm's Lair, downloaded by you".
3. *ScreenScraper* (tier 2, with keys): media type `manuel` (PDF, by
   region) from the `jeuInfos` call romforge already makes, by ROM hash.
   Fetched by Enrich only when nothing above gave one, at most one per
   game (ES-DE scrapes manuals the same way).

Later, maybe: archive.org's manual collections (the complete English
SNES set, ...). Free and keyless, but matched by title only and each
collection names its files its own way.

Where it lives:

- `Media/<System>/manual/<Title>.pdf`, with its source in `game_meta`.
  Cleanup puts it in the archive's `romforge/` folder (rule 7), or the
  sidecar folder for CHDs.
- Served by the `art://` scheme (`.pdf` added, same roots check). The CSP
  gains `connect-src art:` and `worker-src 'self' blob:` for pdf.js.

The reader:

- *pdf.js* (`pdfjs-dist` 6.x, Apache-2.0), drawing pages on a canvas, the
  same on every system (no reliance on WebKitGTK's own PDF view).
- *Leafing through:* one page or a two-page spread, arrow keys and
  Page Up/Down, a strip of page thumbnails, zoom, fit to width or height.
  Pages render as they are reached, so a 60 MB scan stays quick. Each
  game remembers its last page.
- *Pop-out:* a `WebviewWindow` labelled `manual-<id>` opens the same
  reader alone, so the manual can stay open beside the game. It needs
  `core:webview:allow-create-webview-window`, and a capability listing
  the `manual-*` windows (art scheme only, no other commands).
- *Tab:* "Manual" right after "Details". With no manual: the Vimm button
  (when Hasheous gave an id) and a drop zone. Components `ManualTab`,
  `ManualReader` and `PageStrip`, each under 400 lines (rule 11).
- CLI: `manual show|add|download|remove <game>`, all logic in the core.

*(Built 2026-10-20, first part.)* `manual.rs`, `library/manuals.rs`,
app `manual.rs`, `ManualTab`, `ManualReader`, `PageStrip`:
- **Kept** in a `manuals` table (migration 39) by system key and title:
  path, source, Vimm id, whether Hasheous was asked, last page. Files at
  `<Media>/<System>/manual/<Title>.pdf` (the art cache's `art/` without a
  RomForge folder). A PDF must start with `%PDF`; adding copies it.
- **Hasheous** asked once per game, by each of its files' SHA1, when the
  Manual tab first opens on a game without a manual (not from Enrich);
  the answer, even none, is kept. Checked 2026-10-20: Super Mario World
  (USA) gives `VIMMManualId` 4770.
- **Vimm's Lair** download only on the button (DPI 150 / 300 / 600),
  with a Referer of the manual's page, kept only if it is a PDF; else the
  tab says to download it from their page and drop it on the tab. Not
  tried against the live server (its robots.txt asks for no automated
  fetching, so only a user's click should). The restricted fallback
  window from the plan isn't built: the page opens in the browser
  instead.
- **Reader:** pdf.js 6.4.299 (Apache-2.0), loaded only when a manual is
  opened (the main bundle stays as it was); one page or a spread (cover
  alone, then pairs), fit width or whole page, zoom 50–400%, arrows /
  Page Up/Down / Home / End, a lazily drawn thumbnail strip, the page
  remembered. `art://` serves `.pdf`; the CSP gained `connect-src art:`
  and `worker-src 'self' blob:`. Tried in a dev preview with a generated
  PDF.
- **Not yet:** the pop-out window, ScreenScraper manuals, reading ES-DE's
  and LaunchBox's manual folders in "Import my existing setup", Cleanup
  putting the manual in the archive's `romforge/` folder, pad control of
  the reader.

## Code review follow-ups (2026-10-18)

A full review found about 90 items; all are fixed except one that waits
on someone else:

- **Network.** Redump serves no HTTPS (its server resets TLS), so its DATs
  stay on HTTP until it does.

## Later ideas (parked, do not build yet)

- Online lookups for files that stay unknown: RetroAchievements hashes
  (login token), ScreenScraper (keys); Hasheous needs neither, see
  milestone 11.
- Favourites (play history: milestone 18, 1.6).

- Redump update check without re-downloading (Redump has no version index).
- The regions seed from remustwo for 1G1R (clone lists: 12.2).
- RAR support needs a free pure-Rust decoder; none known yet.
