# romforge in uQ

What this folder adds to the [uQ](https://github.com/asafelobotomy/uq) desktop. romforge's
package installs it unconditionally; without uQ nothing reads it.

| File | Installed to | What it does |
| --- | --- | --- |
| `pages/romforge/` | `/usr/share/uq/settings/pages/romforge/` | The **Game library** page in uQ Settings: a look at your library, with buttons that open romforge, and an oysterAV card |
| `launcher/romforge.provider` | `/usr/share/uq/launcher/` | Typing a game's name in look=see>do_ finds it in your library; Enter starts it |
| `dbus/org.romforge.Provider.service` | `/usr/share/dbus-1/services/` | Lets the session bus start `/usr/bin/romforge-provider` when uQ's launcher opens |
| `integrations/romforge/rules.toml` | `/usr/share/uq/integrations/romforge/` | two rules, "Emulators open full screen" and "More emulators open full screen", on the game workspace, each off until you turn it on |

The package also installs `/usr/bin/romforge-provider` (from `crates/romforge-provider`).

## The launcher provider

`romforge-provider` is a small session-bus service on `org.romforge.Provider` that serves
`org.uq.Launcher1.Provider` at `/org/uq/Launcher1`. It is activatable: uQ asks the bus to start it
when look=see>do_ or the app menu opens (at most once every 30 s), and it exits after 60 s without a
call, but never while a game it started is running. The romforge app does not need to be open.

- **Search.** No prefix: every query of two characters or more. It opens the library the app uses
  (`$ROMFORGE_DB`, else the RomForge folder's, else `~/.local/share/romforge/romforge.db`), read
  as the app's list, and never makes one: without a library there are no rows. Only games Play can
  start are offered, by their clean name ("The Legend of Zelda - A Link to the Past"), with the
  platform and the version (regions, revision, translation, hack) as the subtitle and the box art
  as the icon when it has some. Up to 20 rows, best match first.
- **Play** (`play`, Enter) starts the game as the app's Play does: the emulator romforge picks for
  it (or the one set for that game), romforge's config for it written first, its play time
  recorded when it ends. One game at a time. If it won't start, or stops within seconds, a
  desktop notification says why (uQ itself only says that romforge failed). The hotkeys and the
  radial menu work as they do in the app (Emulator Settings turns them off): the provider reads the
  pad while the game runs, and the romforge app, started as `romforge-app --overlay`, draws the
  ring and quits with the game. Without the app, only the ring is missing.
- **Input checks.** Queries over 256 bytes or with control characters get no rows. `Activate`
  takes only the id of a game in the library that Play may start (never a path, never anything
  run through a shell) and only the `play` action; anything else is `InvalidArgs`. ROM files are
  never touched.
- The provider gets its environment (`WAYLAND_DISPLAY` for the emulator) from the session bus's
  activation environment, which uQ's session sets.

## The Game library page

uQ Settings → Apps → **Game library** shows what romforge has found, and changes nothing. Three tabs:

- **Library**: games, verified games and size; one row per platform with a bar next to the biggest
  one and a **View** button that jumps to that platform's games; the games scanned most recently;
  the RomForge, Source and Library folders.
- **Games**: the list, fifty at a time. Four choices: platform, which games (all, verified,
  incomplete, not identified), sort (title, platform, year, status, either way) and words in the
  title.
- **Needs attention**: platforms whose emulator lacks a required BIOS file, and platforms you have
  games for that have no game list yet.

Every **Open romforge** button starts the app (`romforge.desktop`) through uQ; scanning, tidying,
art, emulators and settings stay there. The page reads two commands, both read-only:
`romforge-cli overview --json` (read again whenever the library file changes) and
`romforge-cli browse PLATFORM STATUS SORT LIMIT OFFSET SEARCH` (every choice a positional argument,
because uQ runs a fixed command line). Their answers are in `tests/uq/fixtures/`, and
`crates/romforge-cli/tests/uq_fixtures.rs` checks they have the shape the commands print.

**oysterAV** (`uq_api = 3`). The Library tab has an oysterAV card: a switch for "Check downloads
with oysterAV" (`romforge-cli settings --scan-downloads true|false`, the one thing the page
changes) and buttons that stream `romforge-cli oysterav [emulators|games] --ndjson`, one line per
place looked at, with the files and threat names under any that oysterAV flagged. oysterAV only
looks: nothing is moved or deleted. The page logic for it is `av.js` (`tests/uq/logic.test.mjs`).

Tests: `node --test 'tests/uq/*.test.mjs'` (logic, manifest, packaging) and
`QML_XHR_ALLOW_FILE_READ=1 QT_QPA_PLATFORM=offscreen qmltestrunner -import tests/uq/stub -input tests/uq`
(the page itself, offscreen, with a fake `ctx`); CI runs both and `uqctl integrations check uq/`.

## The game-mode rule

`rules.toml` offers one window rule, `romforge.emulators`: RetroArch, DuckStation, PPSSPP,
Dolphin, PCSX2, RPCS3 and Azahar open full screen on the workspace named `game`. uqd checks it and
renders it only after you turn it on in uQ Settings → Window rules → "Rules from apps"; turn it off
or undo it there. For a quieter game workspace, set its layout to master with no gaps and no
animations on uQ's Look and Window rules pages.

## How it was tested

- `uqctl integrations check uq/` is clean: 1 provider, 1 service file, 1 rule file.
- `cargo test -p romforge-core uq` covers the matching and ranking, the 20-row cap, which rows are
  offered, and the query and id checks; `cargo test -p romforge-provider` the action check, the
  icon choice and the failed-start notification.
- `crates/romforge-provider/tests/dbus.rs` runs on a private `dbus-daemon`: `Search` and
  `Activate` as uQ calls them against a library made from the repository's fixtures, the error
  names for bad ids and actions, a game started through a stand-in emulator, one game at a time,
  no library at all (nothing found, nothing made), and bus activation of the real binary from the
  packaged service file through to its idle exit.

By hand, with romforge installed:

```sh
busctl --user call org.romforge.Provider /org/uq/Launcher1 org.uq.Launcher1.Provider Search s zelda
busctl --user call org.romforge.Provider /org/uq/Launcher1 org.uq.Launcher1.Provider Activate ss 'g:…' play
```
