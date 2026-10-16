# romforge

Catalog and identify retro ROM collections. Rust core, SQLite, a CLI, and a
Tauri 2 + Svelte desktop app.

**Milestone 1 — scan and identify.** romforge only reads your files. It never
moves, renames, writes or extracts them. See [PLAN.md](PLAN.md) for what
comes next.

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
symlink, but symlinked folders inside it are never followed. Long scans show
progress on stderr; `-q` keeps it quiet.

On your own collection, get DATs first, then scan a folder.

```bash
romforge-cli dat download SNES PlayStation    # or --all for every system
```

Cartridge systems (`[no-intro]` in `dat systems`) get No-Intro DATs from
[libretro-database](https://github.com/libretro/libretro-database)'s mirror,
because DAT-o-MATIC only hands DATs out through its download form. Disc
systems (`[redump]`) get Redump's game and BIOS DATs from redump.org. Run
it again to update; an unchanged DAT is left alone. Downloads are spaced
out per site and never saved to disk; the DAT ends up in the database.

DATs you downloaded yourself import too, as `.dat` or as a zip of one or
many (read in memory): `romforge-cli dat import ~/Downloads/No-Intro*.zip`.
`romforge-cli dat check` compares imported No-Intro DATs with DAT-o-MATIC's
latest versions.

Use `--system NES` (keys: `romforge-cli dat systems`) or `--dat ID` to say
what a folder should contain; without it, every DAT is tried. Without
`ROMFORGE_DB` the database lives at `~/.local/share/romforge/romforge.db`.

## How identification works

- Every file, and every member of a `.zip` or `.7z`, is hashed as a stream
  (CRC32, MD5, SHA1). Archives are never unpacked to disk.
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

One hash may belong to many games; every hit is kept. A DAT's source path,
SHA-256, import time and `<clrmamepro header=…>` hint are recorded.
Importing a new version of a DAT (same name) replaces the old one.

## Desktop app

Needs Node and the [Tauri Linux dependencies](https://tauri.app/start/prerequisites/)
(`webkit2gtk-4.1`). It reads the same database as the CLI.

```bash
cd app && npm install && npm run tauri dev
```

`npm run tauri build -- --no-bundle` builds `target/release/romforge-app`.

## Layout

- `crates/romforge-core` — all logic (library)
- `crates/romforge-cli` — thin CLI
- `app/` — Tauri 2 + Svelte shell
- `fixtures/` — tiny generated test files and DATs. Regenerate with
  `cargo test -p romforge-core --test fixtures -- --ignored regenerate`.

## License

MIT (see [LICENSE](LICENSE)). The systems table is adapted from
[remustwo](https://github.com/asafelobotomy/remustwo) (MIT).
